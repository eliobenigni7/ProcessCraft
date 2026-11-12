//! Compound modeling requests (append, connect) applied through the shared command protocol.
use processcraft_core::{
    bpmn_activity::ModelReference,
    bpmn_data::DataAssociationKind,
    commands::{CommandError, Envelope, Mutation, Request, Session},
    ids::{DocumentId, ElementIdentity, ElementSelector},
    model::*,
};
use serde_json::json;

fn docid() -> DocumentId {
    "11111111-1111-4111-8111-111111111111".parse().unwrap()
}
fn sel(s: &str) -> ElementSelector {
    s.parse().unwrap()
}
fn ident(s: &str) -> ElementIdentity {
    s.parse().unwrap()
}
fn send(s: &mut Session, request: Request) -> Result<(), CommandError> {
    let envelope = Envelope {
        request_id: "t".into(),
        session_id: s.id(),
        expected_generation: None,
    };
    s.apply(envelope, request).map(|_| ())
}
fn batch(s: &mut Session, mutations: Vec<Mutation>) -> Result<(), CommandError> {
    send(s, Request::Batch { mutations })
}
fn session() -> Session {
    let mut s = Session::new(ProjectData::new("Project".into())).unwrap();
    let setup: Request = serde_json::from_str(include_str!(
        "../../../tests/fixtures/commands/procurement.json"
    ))
    .unwrap();
    send(&mut s, setup).unwrap();
    s
}
fn document(s: &Session) -> BpmnDocument {
    (*s.snapshot().project.documents[0]).clone()
}
fn shape(s: &Session, element: &str) -> Bounds {
    document(s).diagrams[0]
        .shapes
        .iter()
        .find(|x| x.element == sel(element))
        .unwrap()
        .bounds
        .clone()
}
fn edge_points(s: &Session, element: &str) -> Vec<(f64, f64)> {
    document(s).diagrams[0]
        .edges
        .iter()
        .find(|e| e.element == sel(element))
        .unwrap()
        .waypoints
        .iter()
        .map(|p| (p.x, p.y))
        .collect()
}
fn append(source: &str, kind: NodeKind, id: &str) -> Mutation {
    Mutation::Append {
        document_id: docid(),
        diagram_id: sel("procurement_diagram"),
        source_id: sel(source),
        kind,
        name: None,
        node_id: ident(id),
        shape_id: ident(&format!("{id}_di")),
        flow_id: ident(&format!("{id}_flow")),
        edge_id: ident(&format!("{id}_flow_di")),
    }
}
fn connect(source: &str, target: &str, id: &str) -> Mutation {
    Mutation::Connect {
        document_id: docid(),
        diagram_id: sel("procurement_diagram"),
        source_id: sel(source),
        target_id: sel(target),
        flow_id: ident(id),
        edge_id: ident(&format!("{id}_di")),
    }
}
fn create(kind: &str, id: &str, bounds: [f64; 4]) -> Vec<Mutation> {
    let mut element = json!({"id": id, "name": null, "documentation": "", "kind": kind, "owner": "", "contact": ""});
    if kind == "DataStoreReference" {
        element["details"] = json!({"type": "dataStoreReference", "data_store_ref": null});
    }
    serde_json::from_value(json!([
        {"type": "createElement", "document_id": docid(), "process_id": "procurement",
         "element": element},
        {"type": "createShape", "document_id": docid(), "diagram_id": "procurement_diagram",
         "shape": {"id": format!("{id}_di"), "element": id, "bounds": {"x": bounds[0], "y": bounds[1], "width": bounds[2], "height": bounds[3]}, "label": null, "style": null}}
    ]))
    .unwrap()
}

#[test]
fn append_auto_places_routes_and_undoes_in_one_step() {
    let mut s = session();
    let before = s.snapshot().content_id;
    batch(&mut s, vec![append("review", NodeKind::Task, "check")]).unwrap();
    // Learned gap 60 (review -> decision); the decision blocks the row, so step down by
    // max(blocker bottom + 30, previous + 80 + 30).
    assert_eq!(
        shape(&s, "check"),
        Bounds {
            x: 390.0,
            y: 198.0,
            width: 100.0,
            height: 80.0
        }
    );
    let points = edge_points(&s, "check_flow");
    assert_eq!(points.first(), Some(&(330.0, 128.0)));
    assert_eq!(points.last(), Some(&(390.0, 238.0)));
    assert!(
        document(&s).processes[0]
            .flows
            .iter()
            .any(|f| f.id == sel("check_flow")
                && f.kind == FlowKind::Sequence
                && f.target == Some(ModelReference::Local(sel("check"))))
    );
    send(&mut s, Request::Undo).unwrap();
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn append_annotation_goes_above_right_with_an_association() {
    let mut s = session();
    batch(
        &mut s,
        vec![append("review", NodeKind::TextAnnotation, "note")],
    )
    .unwrap();
    let b = shape(&s, "note");
    assert_eq!((b.x, b.y, b.width, b.height), (330.0, 8.0, 100.0, 30.0));
    let flow = document(&s).processes[0]
        .flows
        .iter()
        .find(|f| f.id == sel("note_flow"))
        .cloned()
        .unwrap();
    assert_eq!(flow.kind, FlowKind::Association);
}

#[test]
fn appended_annotation_name_becomes_its_text() {
    let mut s = session();
    let mut mutation = append("review", NodeKind::TextAnnotation, "note");
    if let Mutation::Append { name, .. } = &mut mutation {
        *name = Some("Within 2 hours".into());
    }
    batch(&mut s, vec![mutation]).unwrap();
    let node = document(&s).processes[0]
        .nodes
        .iter()
        .find(|n| n.id == sel("note"))
        .cloned()
        .unwrap();
    assert_eq!(node.name, None);
    let Some(processcraft_core::bpmn_model::NodeDetails::TextAnnotation { text, .. }) =
        node.details
    else {
        panic!("annotation details missing")
    };
    assert_eq!(text, Some(String::from("Within 2 hours").into()));
}

#[test]
fn backward_sequence_flow_loops_below_into_the_target_bottom() {
    use processcraft_core::geometry::{RouteLayout, layout_route};
    let target = Bounds {
        x: 100.0,
        y: 100.0,
        width: 100.0,
        height: 80.0,
    };
    let source = Bounds {
        x: 400.0,
        y: 200.0,
        width: 100.0,
        height: 80.0,
    };
    let points: Vec<_> = layout_route(&source, &target, &[], RouteLayout::Sequence)
        .iter()
        .map(|p| (p.x, p.y))
        .collect();
    assert_eq!(
        points,
        vec![
            (450.0, 280.0),
            (450.0, 300.0),
            (150.0, 300.0),
            (150.0, 180.0)
        ]
    );
}

#[test]
fn append_and_connect_refuse_with_the_rule_reason() {
    let mut s = session();
    let before = s.snapshot().content_id;
    let error = batch(&mut s, vec![append("end", NodeKind::Task, "after")]).unwrap_err();
    assert!(matches!(&error, CommandError::InvalidParams(m) if m.contains("End events")));
    let error = batch(&mut s, vec![connect("order", "start", "back")]).unwrap_err();
    assert!(matches!(&error, CommandError::InvalidParams(m) if m.contains("Start events")));
    assert_eq!(s.snapshot().content_id, before);
}

#[test]
fn connect_binds_data_associations_to_new_ports() {
    let mut s = session();
    let mut setup = create("DataObjectReference", "form", [200.0, 300.0, 36.0, 50.0]);
    setup.extend(create(
        "DataStoreReference",
        "ledger",
        [700.0, 420.0, 50.0, 50.0],
    ));
    batch(&mut s, setup).unwrap();
    batch(
        &mut s,
        vec![
            connect("form", "review", "readForm"),
            connect("order", "ledger", "writeLedger"),
        ],
    )
    .unwrap();
    let d = document(&s);
    let associations = &d.processes[0].data_associations;
    let input = associations
        .iter()
        .find(|a| a.id == Some(ident("readForm")))
        .unwrap();
    assert_eq!(input.kind, DataAssociationKind::Input);
    assert_eq!(input.sources, vec![sel("form")]);
    assert_eq!(input.target, sel("readForm_port"));
    let output = associations
        .iter()
        .find(|a| a.id == Some(ident("writeLedger")))
        .unwrap();
    assert_eq!(output.kind, DataAssociationKind::Output);
    assert_eq!(output.target, sel("ledger"));
    let review = d.processes[0]
        .nodes
        .iter()
        .find(|n| n.id == sel("review"))
        .unwrap();
    assert_eq!(
        review.io_specification.as_ref().unwrap().inputs[0].id,
        Some(ident("readForm_port"))
    );
    assert_eq!(d.diagrams[0].edges.len(), 7);
    // Port ends have no shape: the edges name the visible shapes so BPMN export resolves them.
    let edge_ends = |flow: &str| {
        let e = d.diagrams[0]
            .edges
            .iter()
            .find(|e| e.element == sel(flow))
            .unwrap();
        (e.source_element.clone(), e.target_element.clone())
    };
    assert_eq!(
        edge_ends("readForm"),
        (Some(sel("form_di")), Some(sel("pc_shape_review")))
    );
    assert!(matches!(edge_ends("writeLedger"), (Some(_), Some(_))));
}

#[test]
fn connect_across_pools_creates_a_message_flow() {
    let mut s = session();
    let mut setup: Vec<Mutation> = serde_json::from_value(json!([
        {"type": "createProcess", "document_id": docid(), "process": {"id": "supplier", "name": "Supplier", "documentation": "", "nodes": [], "lanes": [], "flows": []}},
        {"type": "setCollaborationRoot", "document_id": docid(), "collaboration": {"id": "collab", "participants": [
            {"id": "buyerPool", "name": "Buyer", "process": "procurement"},
            {"id": "supplierPool", "name": "Supplier", "process": "supplier"}
        ], "flows": []}},
        {"type": "setDiagramPlane", "document_id": docid(), "diagram_id": "procurement_diagram", "plane": "collab"},
        {"type": "createElement", "document_id": docid(), "process_id": "supplier",
         "element": {"id": "fulfil", "name": "Fulfil", "documentation": "", "kind": "Task", "owner": "", "contact": ""}},
        {"type": "createShape", "document_id": docid(), "diagram_id": "procurement_diagram",
         "shape": {"id": "fulfil_di", "element": "fulfil", "bounds": {"x": 540.0, "y": 500.0, "width": 100.0, "height": 80.0}, "label": null, "style": null}}
    ]))
    .unwrap();
    setup.push(connect("order", "fulfil", "po"));
    batch(&mut s, setup).unwrap();
    let flow = &document(&s).collaboration.unwrap().flows[0];
    assert_eq!(flow.kind, FlowKind::Message);
    assert_eq!(flow.source, Some(ModelReference::Local(sel("order"))));
    // Shapes overlapping horizontally get one straight vertical segment.
    assert_eq!(edge_points(&s, "po"), vec![(590.0, 350.0), (590.0, 500.0)]);
}
