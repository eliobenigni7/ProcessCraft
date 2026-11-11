use processcraft_core::{bpmn_catalog::{BpmnResource, ItemDefinition, ResourceParameter}, ids::ElementKey, model::*};
use std::sync::Arc;
fn main() {
    let mut p = ProjectData::new("Synthetic compatibility baseline".into());
    p.id = "11111111-1111-4111-8111-111111111111".parse().unwrap();
    let mut d = BpmnDocument::new("Example".into(), "definitions".parse().unwrap());
    d.id = "22222222-2222-4222-8222-222222222222".parse().unwrap();
    d.processes.push(Process { id: "process".parse().unwrap(), name: "Review".into(), documentation: String::new(), lane_set_id: None, groups: vec![], data_objects: vec![], data_associations: vec![], nodes: vec![Node { id: "task".parse().unwrap(), name: "Assess".into(), documentation: String::new(), kind: NodeKind::Task, details: None, loop_characteristics: None, io_specification: None, activity: None, owner: String::new(), contact: String::new() }], lanes: vec![], flows: vec![] });
    d.diagrams.push(Diagram { id: "diagram".parse().unwrap(), plane_id: "plane".parse().unwrap(), plane: "process".parse().unwrap(), shapes: vec![Shape { id: "shape".parse().unwrap(), element: "task".parse().unwrap(), is_expanded: None, bounds: Bounds { x: 20.0, y: 30.0, width: 100.0, height: 80.0 }, label: None, style: None }], edges: vec![] });
    d.catalog.item_definitions.push(ItemDefinition { id: None, structure_ref: None, is_collection: None, item_kind: None });
    d.catalog.resources.push(BpmnResource { id: None, name: "Reviewer".into(), parameters: vec![ResourceParameter { id: None, name: None, type_ref: None, is_required: None }] });
    let key = ElementKey { document_id: d.id, xml_id: "task".parse().unwrap() };
    p.documents.push(Arc::new(d));
    let entry = "33333333-3333-4333-8333-333333333333".parse().unwrap();
    p.dictionary_entries.push(DictionaryEntry { id: entry, kind: DictionaryKind::System, name: "System".into(), description: "Synthetic".into() });
    p.dictionary_links.push(DictionaryLink { source: key.clone(), kind: DictionaryKind::System, target: DictionaryTarget::Entry(entry) });
    p.comments.push(Comment { id: "44444444-4444-4444-8444-444444444444".parse().unwrap(), target: key, author: "Example".into(), text: "Synthetic comment".into() });
    let checkpoint = "55555555-5555-4555-8555-555555555555".parse().unwrap();
    p.checkpoints.push(Checkpoint { id: checkpoint, name: "Before rename".into(), created_unix_ms: 1700000000000, content: FrozenProject::capture(&p) });
    p.reference_checkpoint = Some(checkpoint);
    p.name = "Synthetic compatibility baseline current".into();
    let id = p.content_identity().unwrap();
    let bytes = processcraft_io::native::write_bytes(&p).unwrap();
    std::fs::write("/workspace/work/identity-baseline.processcraft", bytes).unwrap();
    println!("{}", id.iter().map(|b| format!("{b:02x}")).collect::<String>());
}
