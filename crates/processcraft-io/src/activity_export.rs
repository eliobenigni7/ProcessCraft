//! Read-only XML admission checks for the expanded activity family.
//! Missing required XML values stay editable native data; output never invents them.
use processcraft_core::{
    bpmn_activity::*,
    bpmn_data::{IoSet, IoSpecification},
    bpmn_model::{EventDefinition, EventReference, LoopCharacteristics, NodeDetails},
    ids::ElementSelector,
    model::{BpmnDocument, DefinitionsMetadata, GENERATED_TARGET_NAMESPACE, Node},
};
use std::{borrow::Cow, collections::HashMap};

struct Check {
    xml_ids: HashMap<ElementSelector, bool>,
}
fn required<T>(value: &Option<T>, path: &str) -> Result<(), String> {
    value
        .as_ref()
        .map(|_| ())
        .ok_or_else(|| format!("{path} is required for BPMN XML export"))
}

fn retrieval_namespace_context(
    reference: &ModelReference,
    expression: &processcraft_core::bpmn_model::Expression,
    namespace: &str,
    path: &str,
) -> Result<(), String> {
    const BPMN: &str = "http://www.omg.org/spec/BPMN/20100524/MODEL";
    let reference_namespace = match reference {
        ModelReference::Local(_) => namespace,
        ModelReference::Qualified(qname) => &qname.namespace,
    };
    if reference_namespace == BPMN {
        return Ok(());
    }
    if let processcraft_core::bpmn_model::Expression::Structured(data) = expression
        && data
            .namespaces
            .iter()
            .any(|(prefix, uri)| prefix.is_empty() && uri == BPMN)
        && data
            .namespaces
            .iter()
            .all(|(prefix, _)| prefix.is_empty() || prefix == "xml")
    {
        // A QName on the enclosing BPMN owner needs a prefix here. XML 1.0
        // cannot undeclare that prefix on the child, whose captured scope has
        // none. Keep the source context intact and explain the required edit.
        return Err(format!(
            "{path}: the message reference requires a namespace prefix that is absent from the captured expression context. Add an appropriate namespace binding in the expression editor before BPMN XML export"
        ));
    }
    Ok(())
}
impl Check {
    fn selector(&self, value: &ElementSelector, path: &str) -> Result<(), String> {
        match self.xml_ids.get(value) {
            Some(true) => Ok(()),
            Some(false) => Err(format!(
                "Assign an XML ID to the local target of {path} before BPMN XML export"
            )),
            None => Err(format!("The local target of {path} does not exist")),
        }
    }
    fn set(&self, set: &IoSet, path: &str) -> Result<(), String> {
        for (field, values) in [
            ("data_refs", &set.data_refs),
            ("optional_refs", &set.optional_refs),
            ("while_executing_refs", &set.while_executing_refs),
            ("counterpart_refs", &set.counterpart_refs),
        ] {
            for value in values {
                self.selector(value, &format!("{path}.{field}"))?;
            }
        }
        Ok(())
    }
    fn io(&self, io: &IoSpecification, path: &str) -> Result<(), String> {
        if io.input_sets.is_empty() || io.output_sets.is_empty() {
            return Err(format!(
                "{path} requires at least one input set and one output set for BPMN XML export"
            ));
        }
        for (field, sets) in [
            ("input_sets", &io.input_sets),
            ("output_sets", &io.output_sets),
        ] {
            for (index, set) in sets.iter().enumerate() {
                self.set(set, &format!("{path}.{field}[{index}]"))?;
            }
        }
        Ok(())
    }
    fn callable(&self, value: &CallableMetadata, path: &str) -> Result<(), String> {
        for (index, binding) in value.io_bindings.iter().enumerate() {
            let path = format!("{path}.io_bindings[{index}]");
            required(&binding.operation_ref, &format!("{path}.operation_ref"))?;
            required(&binding.input_data_ref, &format!("{path}.input_data_ref"))?;
            required(&binding.output_data_ref, &format!("{path}.output_data_ref"))?;
            self.selector(
                binding.input_data_ref.as_ref().unwrap(),
                &format!("{path}.input_data_ref"),
            )?;
            self.selector(
                binding.output_data_ref.as_ref().unwrap(),
                &format!("{path}.output_data_ref"),
            )?;
        }
        Ok(())
    }
    fn roles(&self, values: &[ResourceRole], path: &str) -> Result<(), String> {
        for (index, role) in values.iter().enumerate() {
            let path = format!("{path}[{index}]");
            match &role.assignment {
                ResourceRoleAssignment::None => {}
                ResourceRoleAssignment::Resource { bindings, .. } => {
                    for (index, binding) in bindings.iter().enumerate() {
                        let path = format!("{path}.bindings[{index}]");
                        required(&binding.parameter_ref, &format!("{path}.parameter_ref"))?;
                        required(&binding.expression, &format!("{path}.expression"))?;
                    }
                }
                ResourceRoleAssignment::Expression { assignment } => required(
                    &assignment.expression,
                    &format!("{path}.assignment.expression"),
                )?,
            }
        }
        Ok(())
    }
    fn implicit_event(&self, event: &ImplicitThrowEvent, path: &str) -> Result<(), String> {
        for (index, association) in event.input_associations.iter().enumerate() {
            let path = format!("{path}.input_associations[{index}]");
            required(&association.target, &format!("{path}.target"))?;
            self.selector(
                association.target.as_ref().unwrap(),
                &format!("{path}.target"),
            )?;
            for source in &association.sources {
                self.selector(source, &format!("{path}.sources"))?;
            }
        }
        if let Some(set) = &event.input_set {
            self.set(set, &format!("{path}.input_set"))?;
        }
        // Definition context semantics remain separate from required XML grammar.
        // The existing event writer handles the already-supported definition fields.
        for definition in &event.definitions {
            if let EventDefinition::Timer {
                kind, expression, ..
            } = definition
                && kind.is_some() != expression.is_some()
            {
                return Err(format!(
                    "{path} timer kind and expression must be supplied together"
                ));
            }
        }
        Ok(())
    }
    fn scope(&self, nodes: &[Node], path: &str) -> Result<(), String> {
        for (index, node) in nodes.iter().enumerate() {
            let path = format!("{path}.nodes[{index}]");
            if let Some(io) = &node.io_specification {
                self.io(io, &format!("{path}.io_specification"))?;
            }
            if let Some(activity) = &node.activity {
                self.roles(&activity.resource_roles, &format!("{path}.resource_roles"))?;
            }
            if let Some(LoopCharacteristics::MultiInstance {
                complex_behavior_definitions,
                ..
            }) = &node.loop_characteristics
            {
                for (index, behavior) in complex_behavior_definitions.iter().enumerate() {
                    let path = format!("{path}.complex_behavior_definitions[{index}]");
                    required(&behavior.condition, &format!("{path}.condition"))?;
                    if let Some(event) = &behavior.event {
                        self.implicit_event(event, &format!("{path}.event"))?;
                    }
                }
            }
            if let Some(NodeDetails::SubProcess { content }) = &node.details {
                self.scope(&content.nodes, &path)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn validate(document: &BpmnDocument, effective_namespace: &str) -> Result<(), String> {
    if document
        .definitions
        .as_ref()
        .is_some_and(|value| value.target_namespace != effective_namespace)
    {
        return Err("Export namespace disagrees with Definitions metadata".into());
    }
    // Old archives may retain their authoritative namespace solely in provenance.
    // Supply the export boundary's recovered context to the read-only scanner,
    // without changing the native document or interpreting the retained source.
    let checked =
        if document.definitions.is_none() && effective_namespace != GENERATED_TARGET_NAMESPACE {
            let mut copy = document.clone();
            copy.definitions = Some(DefinitionsMetadata {
                namespaces: vec![],
                foreign_attributes: vec![],
                target_namespace: effective_namespace.into(),
                imports: vec![],
                name: None,
                expression_language: None,
                type_language: None,
                exporter: None,
                exporter_version: None,
            });
            Cow::Owned(copy)
        } else {
            Cow::Borrowed(document)
        };
    crate::collaboration_export::validate(&checked)?;
    let check = Check {
        xml_ids: checked
            .element_identities()
            .into_iter()
            .map(|id| (id.selector(), id.xml_id().is_some()))
            .collect(),
    };
    if let Some(issue) =
        processcraft_core::validation::activity_family_reference_issues(&checked).first()
    {
        let owner = processcraft_core::queries::element_name(&checked, &issue.owner)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .or_else(|| {
                checked
                    .element_identities()
                    .into_iter()
                    .find(|identity| identity.selector() == issue.owner)
                    .and_then(|identity| identity.xml_id().map(ToString::to_string))
            })
            .unwrap_or_else(|| "an anonymous model element".into());
        return Err(format!(
            "{} has an unresolved, wrong-kind or out-of-scope local target on {owner}",
            issue.field
        ));
    }
    for resolution in processcraft_core::validation::activity_family_reference_resolutions(&checked)
    {
        // Plane order is native representation bookkeeping, not an XML IDREF.
        // XML emits children in that order without referencing their XML IDs.
        if !matches!(
            resolution.reference.field,
            "di.planeOrder" | "definitions.rootOrder"
        ) && matches!(resolution.reference.target, EventReference::Local(_))
        {
            check.selector(&resolution.resolved_target, resolution.reference.field)?;
        }
    }
    for (index, property) in checked.correlation_properties.iter().enumerate() {
        let path = format!("correlation_properties[{index}]");
        if property.retrieval_expressions.is_empty() {
            return Err(format!(
                "{path} requires a retrieval expression for BPMN XML export"
            ));
        }
        for (index, retrieval) in property.retrieval_expressions.iter().enumerate() {
            let path = format!("{path}.retrieval_expressions[{index}]");
            required(&retrieval.message_ref, &format!("{path}.message_ref"))?;
            required(&retrieval.message_path, &format!("{path}.message_path"))?;
            retrieval_namespace_context(
                retrieval.message_ref.as_ref().unwrap(),
                retrieval.message_path.as_ref().unwrap(),
                effective_namespace,
                &format!("{path}.message_path"),
            )?;
        }
    }
    for (index, global) in checked.global_tasks.iter().enumerate() {
        let path = format!("global_tasks[{index}]");
        check.callable(&global.callable, &format!("{path}.callable"))?;
        if let Some(io) = &global.io_specification {
            check.io(io, &format!("{path}.io_specification"))?;
        }
        check.roles(&global.resource_roles, &format!("{path}.resource_roles"))?;
        if matches!(global.kind, GlobalTaskKind::CallableElement)
            && !global.resource_roles.is_empty()
        {
            return Err(format!(
                "{path}: a generic callable element cannot contain task resource roles"
            ));
        }
    }
    for (index, process) in checked.processes.iter().enumerate() {
        let path = format!("processes[{index}]");
        if let Some(io) = &process.io_specification {
            check.io(io, &format!("{path}.io_specification"))?;
        }
        if let Some(activity) = &process.activity_metadata {
            check.callable(&activity.callable, &format!("{path}.callable"))?;
            check.roles(&activity.resource_roles, &format!("{path}.resource_roles"))?;
            for (index, subscription) in activity.correlation_subscriptions.iter().enumerate() {
                let path = format!("{path}.correlation_subscriptions[{index}]");
                required(&subscription.key_ref, &format!("{path}.key_ref"))?;
                for (index, binding) in subscription.bindings.iter().enumerate() {
                    let path = format!("{path}.bindings[{index}]");
                    required(&binding.property_ref, &format!("{path}.property_ref"))?;
                    required(&binding.data_path, &format!("{path}.data_path"))?;
                }
            }
        }
        check.scope(&process.nodes, &path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use processcraft_core::{bpmn_data::DataPort, bpmn_model::QNameRef, ids::ElementIdentity};
    fn document() -> BpmnDocument {
        BpmnDocument::new("Model".into(), "Definitions".parse().unwrap())
    }
    fn external() -> ModelReference {
        ModelReference::Qualified(QNameRef {
            namespace: "urn:external".into(),
            local: "Operation".parse().unwrap(),
        })
    }
    #[test]
    fn incomplete_correlation_is_refused_without_modifying_native_values() {
        let mut doc = document();
        doc.correlation_properties.push(CorrelationProperty {
            id: None,
            base: None,
            name: None,
            item_type: None,
            retrieval_expressions: vec![],
        });
        assert!(
            validate(&doc, GENERATED_TARGET_NAMESPACE)
                .unwrap_err()
                .contains("retrieval expression")
        );
        doc.correlation_properties[0]
            .retrieval_expressions
            .push(CorrelationRetrieval {
                id: None,
                base: None,
                message_ref: None,
                message_path: None,
            });
        assert!(
            validate(&doc, GENERATED_TARGET_NAMESPACE)
                .unwrap_err()
                .contains("message_ref")
        );
        doc.correlation_properties[0].retrieval_expressions[0].message_ref = Some(external());
        assert!(
            validate(&doc, GENERATED_TARGET_NAMESPACE)
                .unwrap_err()
                .contains("message_path")
        );
        doc.correlation_properties[0].retrieval_expressions[0].message_path =
            Some(processcraft_core::bpmn_model::Expression::Structured(
                processcraft_core::bpmn_model::ExpressionData {
                    id: None,
                    kind: processcraft_core::bpmn_model::ExpressionKind::Formal {
                        explicit_type: false,
                        language: None,
                        evaluates_to_type_ref: None,
                    },
                    text: "inert".into(),
                    foreign_attributes: vec![],
                    namespaces: vec![
                        (
                            String::new(),
                            "http://www.omg.org/spec/BPMN/20100524/MODEL".into(),
                        ),
                        ("external".into(), "urn:external".into()),
                    ],
                },
            ));
        let before = serde_json::to_value(&doc).unwrap();
        validate(&doc, GENERATED_TARGET_NAMESPACE).unwrap();
        assert_eq!(serde_json::to_value(&doc).unwrap(), before);
    }
    #[test]
    fn callable_binding_requires_real_xml_ids_and_preserves_external_qnames() {
        let mut doc = document();
        let mut input = ElementIdentity::anonymous();
        if let ElementIdentity::Native { xml_id, .. } = &mut input {
            *xml_id = Some("Input".parse().unwrap());
        }
        let input_selector = input.selector();
        doc.global_tasks.push(GlobalTask {
            id: None,
            base: None,
            name: None,
            kind: GlobalTaskKind::Task,
            resource_roles: vec![],
            callable: CallableMetadata {
                supported_interface_refs: vec![],
                io_bindings: vec![IoBinding {
                    id: None,
                    base: None,
                    operation_ref: None,
                    input_data_ref: Some(input_selector),
                    output_data_ref: Some("Output".parse().unwrap()),
                }],
            },
            io_specification: Some(IoSpecification {
                inputs: vec![DataPort {
                    id: Some(input),
                    base: None,
                    name: None,
                    item_aware: None,
                    is_collection: None,
                }],
                outputs: vec![DataPort {
                    id: Some("Output".parse().unwrap()),
                    base: None,
                    name: None,
                    item_aware: None,
                    is_collection: None,
                }],
                input_sets: vec![IoSet::default()],
                output_sets: vec![IoSet::default()],
                ..Default::default()
            }),
        });
        assert!(
            validate(&doc, GENERATED_TARGET_NAMESPACE)
                .unwrap_err()
                .contains("operation_ref")
        );
        doc.global_tasks[0].callable.io_bindings[0].operation_ref = Some(external());
        if let Some(ElementIdentity::Native { xml_id, .. }) = doc.global_tasks[0]
            .io_specification
            .as_mut()
            .unwrap()
            .inputs[0]
            .id
            .as_mut()
        {
            *xml_id = None;
        }
        let before = serde_json::to_value(&doc).unwrap();
        assert!(
            validate(&doc, GENERATED_TARGET_NAMESPACE)
                .unwrap_err()
                .contains("Assign an XML ID")
        );
        assert_eq!(serde_json::to_value(&doc).unwrap(), before);
        let Some(ElementIdentity::Native { xml_id, .. }) = doc.global_tasks[0]
            .io_specification
            .as_mut()
            .unwrap()
            .inputs[0]
            .id
            .as_mut()
        else {
            panic!("native input")
        };
        *xml_id = Some("Input".parse().unwrap());
        validate(&doc, GENERATED_TARGET_NAMESPACE).unwrap();
    }
    #[test]
    fn role_expression_and_implicit_association_targets_are_required() {
        let check = Check {
            xml_ids: HashMap::new(),
        };
        let role = ResourceRole {
            id: None,
            base: None,
            name: None,
            kind: ResourceRoleKind::Performer,
            assignment: ResourceRoleAssignment::Expression {
                assignment: ResourceAssignmentExpression {
                    id: None,
                    base: None,
                    expression: None,
                },
            },
        };
        assert!(
            check
                .roles(&[role], "roles")
                .unwrap_err()
                .contains("assignment.expression")
        );
        let event = ImplicitThrowEvent {
            id: None,
            base: None,
            name: None,
            flow: None,
            properties: vec![],
            incoming: vec![],
            outgoing: vec![],
            inputs: vec![],
            input_set: None,
            definitions: vec![],
            definition_refs: vec![],
            input_associations: vec![ImplicitInputAssociation {
                id: None,
                base: None,
                sources: vec![],
                target: None,
                transformation: None,
                assignments: vec![],
            }],
        };
        assert!(
            check
                .implicit_event(&event, "event")
                .unwrap_err()
                .contains("target is required")
        );
    }
}
