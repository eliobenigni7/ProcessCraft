//! Local message and operation reference checks shared by the catalog and activity mutations.
use super::*;
pub(super) fn has_qualified_references(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| {
        matches!(
            node.details,
            Some(crate::bpmn_model::NodeDetails::CallActivity { .. })
        ) || node.activity.as_ref().is_some_and(|activity| {
            activity.message_ref.is_some() || activity.operation_ref.is_some()
        }) || match &node.details {
            Some(crate::bpmn_model::NodeDetails::SubProcess { content }) => {
                has_qualified_references(&content.nodes)
            }
            _ => false,
        }
    })
}
pub(super) fn local_operation_exists(
    d: &BpmnDocument,
    reference: &crate::bpmn_model::QNameRef,
    catalog: &crate::bpmn_catalog::SupportingCatalog,
) -> bool {
    let namespace = d
        .definitions
        .as_ref()
        .map(|m| m.target_namespace.as_str())
        .or_else(|| d.provenance.is_none().then_some(GENERATED_TARGET_NAMESPACE));
    namespace != Some(reference.namespace.as_str())
        || catalog
            .interfaces
            .iter()
            .flat_map(|i| &i.operations)
            .any(|o| {
                o.id.as_ref()
                    .is_some_and(|id| id.xml_id() == Some(&reference.local))
            })
}
pub(super) fn local_message_exists(
    d: &BpmnDocument,
    reference: &crate::bpmn_model::QNameRef,
    resources: &[crate::bpmn_model::EventResource],
) -> bool {
    let namespace = d
        .definitions
        .as_ref()
        .map(|m| m.target_namespace.as_str())
        .or_else(|| d.provenance.is_none().then_some(GENERATED_TARGET_NAMESPACE));
    namespace != Some(reference.namespace.as_str()) || resources.iter().any(|resource| matches!(resource, crate::bpmn_model::EventResource::Message { id, .. } if id.xml_id() == Some(&reference.local)))
}
pub(super) fn check_activity_references(
    d: &BpmnDocument,
    activity: &crate::bpmn_activity::ActivityProperties,
    previous: Option<&crate::bpmn_activity::ActivityProperties>,
) -> Result<(), CommandError> {
    let issues = crate::validation::activity_reference_issues(d, activity);
    for (current, old) in [
        (
            &activity.message_ref,
            previous.and_then(|p| p.message_ref.as_ref()),
        ),
        (
            &activity.operation_ref,
            previous.and_then(|p| p.operation_ref.as_ref()),
        ),
    ] {
        if let Some(name) = current
            && Some(name) != old
            && crate::validation::invalid_activity_namespace(&name.namespace)
        {
            return Err(invalid("Invalid activity QName namespace"));
        }
    }
    if issues.contains(&"missingLocalActivityMessage")
        && previous.map(|p| &p.message_ref) != Some(&activity.message_ref)
    {
        return Err(invalid("Activity references a missing local message"));
    }
    if issues.contains(&"missingLocalActivityOperation")
        && previous.map(|p| &p.operation_ref) != Some(&activity.operation_ref)
    {
        return Err(invalid("Activity references a missing local operation"));
    }
    Ok(())
}
