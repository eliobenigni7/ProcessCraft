//! Parsers for supporting catalog items: item definitions, interfaces and resources.
use crate::interchange::import::metadata::take_anonymous_base;
use crate::interchange::import::tree::{Elem, attrs, optional_bool, optional_id};
use crate::ns::BPMN;
use processcraft_core::bpmn_catalog::{
    BpmnResource, Interface, ItemDefinition, ItemKind, Operation, ResourceParameter,
};
pub(super) fn catalog_item(e: &Elem) -> Option<ItemDefinition> {
    if !attrs(e, &["id", "structureRef", "isCollection", "itemKind"]) || !e.children.is_empty() {
        return None;
    }
    Some(ItemDefinition {
        base: take_anonymous_base(e),
        id: optional_id(e)?,
        structure_ref: e.catalog_qnames.get("structureRef").cloned(),
        is_collection: optional_bool(e.attrs.get("isCollection"))?,
        item_kind: match e.attrs.get("itemKind").map(String::as_str) {
            None => None,
            Some("Information") => Some(ItemKind::Information),
            Some("Physical") => Some(ItemKind::Physical),
            _ => return None,
        },
    })
}
pub(super) fn catalog_interface(e: &Elem) -> Option<Interface> {
    if !attrs(e, &["id", "name", "implementationRef"])
        || e.children.is_empty()
        || e.children.len() > 256
    {
        return None;
    }
    let mut operations = Vec::new();
    for op in &e.children {
        if op.ns != BPMN
            || op.name != "operation"
            || !attrs(op, &["id", "name", "implementationRef"])
        {
            return None;
        }
        let mut input = None;
        let mut output = None;
        let mut errors = Vec::new();
        for child in &op.children {
            if child.ns != BPMN || !child.attrs.is_empty() || !child.children.is_empty() {
                return None;
            }
            let q = child.catalog_text_qname.clone()?;
            match child.name.as_str() {
                "inMessageRef" if input.is_none() && output.is_none() && errors.is_empty() => {
                    input = Some(q)
                }
                "outMessageRef" if input.is_some() && output.is_none() && errors.is_empty() => {
                    output = Some(q)
                }
                "errorRef" if input.is_some() && errors.len() < 256 => errors.push(q),
                _ => return None,
            }
        }
        operations.push(Operation {
            base: take_anonymous_base(op),
            id: optional_id(op)?,
            name: op.attrs.get("name")?.clone(),
            implementation_ref: op.catalog_qnames.get("implementationRef").cloned(),
            in_message_ref: input?,
            out_message_ref: output,
            error_refs: errors,
        });
    }
    Some(Interface {
        base: take_anonymous_base(e),
        id: optional_id(e)?,
        name: e.attrs.get("name")?.clone(),
        implementation_ref: e.catalog_qnames.get("implementationRef").cloned(),
        operations,
    })
}
pub(super) fn catalog_resource(e: &Elem) -> Option<BpmnResource> {
    if !attrs(e, &["id", "name"]) || e.children.len() > 256 {
        return None;
    }
    let mut parameters = Vec::new();
    for p in &e.children {
        if p.ns != BPMN
            || p.name != "resourceParameter"
            || !attrs(p, &["id", "name", "type", "isRequired"])
            || !p.children.is_empty()
        {
            return None;
        }
        parameters.push(ResourceParameter {
            base: take_anonymous_base(p),
            id: optional_id(p)?,
            name: p.attrs.get("name").cloned(),
            type_ref: p.catalog_qnames.get("type").cloned(),
            is_required: optional_bool(p.attrs.get("isRequired"))?,
        });
    }
    Some(BpmnResource {
        base: take_anonymous_base(e),
        id: optional_id(e)?,
        name: e.attrs.get("name")?.clone(),
        parameters,
    })
}
