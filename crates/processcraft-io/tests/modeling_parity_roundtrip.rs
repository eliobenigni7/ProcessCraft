//! BPMN export and re-import of the connections the modeling rules now allow: compensation
//! associations, associations across scopes and to pools, self-loops, and message flows to
//! nodes nested in a subprocess.
use processcraft_core::{
    bpmn_model::AssociationDirection,
    commands::{Envelope, Mutation, Request, Session},
    model::{FlowKind, ProjectData},
    queries::find_flow,
    replace::{ReplaceTarget, Trigger},
};
use serde_json::json;

const DOC: &str = "11111111-1111-4111-8111-111111111111";
const DIAGRAM: &str = "procurement_diagram";

fn send(s: &mut Session, mutations: Vec<Mutation>) {
    let envelope = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(envelope, Request::Batch { mutations }).unwrap();
}
fn run(s: &mut Session, value: serde_json::Value) {
    send(s, serde_json::from_value(value).unwrap());
}
fn create_at(kind: &str, id: &str, x: f64, y: f64) -> serde_json::Value {
    json!({"type": "createAt", "document_id": DOC, "diagram_id": DIAGRAM, "kind": kind,
           "name": null, "center": {"x": x, "y": y}, "node_id": id, "shape_id": format!("{id}_di")})
}
fn connect(source: &str, target: &str, id: &str) -> serde_json::Value {
    json!({"type": "connect", "document_id": DOC, "diagram_id": DIAGRAM,
           "source_id": source, "target_id": target, "flow_id": id, "edge_id": format!("{id}_di")})
}

#[test]
fn new_connection_kinds_survive_export_and_reimport() {
    let mut s = Session::new(ProjectData::new("Project".into())).unwrap();
    let setup: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    let Request::Batch { mutations } = setup else {
        unreachable!()
    };
    send(&mut s, mutations);
    run(
        &mut s,
        json!([{"type": "createPool", "document_id": DOC, "diagram_id": DIAGRAM, "name": "Buyer"}]),
    );
    run(
        &mut s,
        json!([{"type": "createPool", "document_id": DOC, "diagram_id": DIAGRAM,
                "name": "Supplier", "center": {"x": 600.0, "y": 800.0}}]),
    );
    run(
        &mut s,
        json!([
            create_at("SubProcess", "sub", 700.0, 800.0),
            create_at("Task", "inner", 650.0, 800.0),
            create_at("TextAnnotation", "note", 700.0, 180.0),
            create_at("IntermediateCatchEvent", "comp", 260.0, 168.0),
        ]),
    );
    send(
        &mut s,
        vec![Mutation::ReplaceElement {
            document_id: DOC.parse().unwrap(),
            element_id: "comp".parse().unwrap(),
            target: ReplaceTarget::Node {
                kind: processcraft_core::model::NodeKind::BoundaryEvent,
                trigger: Some(Trigger::Compensate),
                non_interrupting: false,
                event_sub_process: false,
                collapsed: false,
            },
        }],
    );
    let supplier = {
        let snapshot = s.snapshot();
        let doc = &snapshot.project.documents[0];
        doc.collaboration.as_ref().unwrap().participants[1]
            .id
            .to_string()
    };
    run(
        &mut s,
        json!([
            connect("comp", "order", "handler"),
            connect("review", "inner", "nested_message"),
            connect("note", "inner", "note_inner"),
            connect("note", supplier.as_str(), "note_pool"),
            connect("review", "review", "loop"),
        ]),
    );
    let project = (*s.snapshot().project).clone();
    let id = project.documents[0].id;
    let output = processcraft_io::interchange::export_bpmn(&project, id, false).unwrap();
    let reopened = processcraft_io::interchange::prepare_import(&output.bytes, "parity").unwrap();
    assert!(
        reopened.report.editable,
        "{:?}",
        reopened.report.limitations
    );
    let doc = &reopened.document;
    let handler = find_flow(doc, &"handler".parse().unwrap()).unwrap();
    assert_eq!(
        handler.association_direction,
        Some(AssociationDirection::One)
    );
    assert_eq!(
        find_flow(doc, &"note_inner".parse().unwrap()).unwrap().kind,
        FlowKind::Association
    );
    assert_eq!(
        find_flow(doc, &"loop".parse().unwrap()).unwrap().kind,
        FlowKind::Sequence
    );
    let root = doc.collaboration.as_ref().unwrap();
    assert!(
        root.flows
            .iter()
            .any(|f| f.id.to_string() == "nested_message")
    );
    assert!(
        root.extras
            .iter()
            .flat_map(|e| &e.artifacts)
            .any(|a| matches!(
                a,
                processcraft_core::bpmn_collaboration::CollaborationArtifact::Association(f)
                    if f.id.to_string() == "note_pool"
            ))
    );
}
