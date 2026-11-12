#[path = "../src/bpmn_scope.rs"]
mod bpmn_scope;

#[test]
fn declarations_are_addressable_without_claiming_support() {
    let inventory = bpmn_scope::inventory();
    assert_eq!(inventory.version, "2.0.2");
    assert!(inventory.declarations.len() >= 500);
    for item in &inventory.declarations {
        assert!(!item.schema.is_empty() && !item.namespace.is_empty() && !item.path.is_empty());
        assert!(
            bpmn_scope::find(&item.namespace, &item.kind, &item.name).any(|d| d.path == item.path)
        );
        for status in [
            &item.coverage.model,
            &item.coverage.edit,
            &item.coverage.render,
            &item.coverage.import,
            &item.coverage.export,
            &item.coverage.validate,
        ] {
            // "verified" is set only from evidence fragments the inventory generator validates.
            assert!(matches!(
                status.as_str(),
                "unverified" | "partial" | "verified"
            ));
        }
    }
}
