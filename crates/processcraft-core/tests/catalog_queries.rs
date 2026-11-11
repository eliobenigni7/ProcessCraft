use processcraft_core::{model::*, queries};
use std::sync::Arc;

#[test]
fn catalog_query_preserves_scalar_values_and_empty_containers_with_json_pointers() {
    let mut project = ProjectData::new("Catalog query".into());
    let mut document = BpmnDocument::new("Model".into(), "Definitions".parse().unwrap());
    document.catalog = serde_json::from_value(serde_json::json!({
        "item_definitions":[{"id":"Item","structure_ref":null,"item_kind":"Physical","is_collection":false}],
        "interfaces":[{"id":null,"name":"Service","implementation_ref":null,"operations":[{
            "id":"Operation","name":"Run","implementation_ref":null,"in_message_ref":{"namespace":"urn:external","local":"Request"},
            "out_message_ref":null,"error_refs":[]
        }]}]
    })).unwrap();
    let id = document.id;
    let expected = serde_json::to_value(&document.catalog).unwrap();
    project.documents.push(Arc::new(document));
    project.validate().unwrap();
    let rows = queries::supporting_catalog_fields(&project, id).unwrap();
    assert!(rows.len() > 10);
    for row in &rows {
        assert_eq!(expected.pointer(&row.path), Some(&row.value));
        assert!(serde_json::to_vec(row).unwrap().len() < 16_384);
    }
    assert!(
        rows.iter()
            .any(|r| r.path.ends_with("/error_refs") && r.value == serde_json::json!([]))
    );
    assert!(
        rows.iter()
            .any(|r| r.path.ends_with("/is_collection") && r.value == false)
    );
    assert!(
        rows.iter()
            .any(|r| r.path.ends_with("/structure_ref") && r.value.is_null())
    );
    assert!(
        queries::supporting_catalog_fields(&project, processcraft_core::ids::DocumentId::new())
            .is_none()
    );
}
