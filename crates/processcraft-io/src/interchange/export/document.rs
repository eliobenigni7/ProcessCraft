//! Whole-document export, reference checks and the round-trip difference report.
use crate::bpmn::import_bpmn;
use crate::interchange::attributes;
use crate::interchange::export::ExportContext;
use crate::interchange::export::choreography::write_choreography_graph;
use crate::interchange::export::collaboration::{
    collaboration_base_scopes, event_reference_qname, local_qname_value,
    write_collaboration_artifacts, write_collaboration_associations,
    write_collaboration_qname_owner, write_collaboration_refs, write_conversation_links,
    write_conversations, write_event_qname_text, write_local_qname_open, write_message_flow,
    write_participant,
};
use crate::interchange::export::elements::{
    anonymous_base_attributes, anonymous_base_children, anonymous_extras_attributes,
    finish_anonymous_base_empty, local_flow_xml_reference, write_audit_monitor,
    write_correlation_key, write_correlation_property, write_correlation_subscription,
    write_event_definition, write_event_property, write_flow_metadata, write_global_task,
    write_io_binding, write_io_specification, write_item_attributes, write_item_state, write_lanes,
    write_resource_role, write_xml_id, write_xml_name, xml_reference,
};
use crate::interchange::export::owner_metadata::{
    collect_owner_metadata, insert_owner_metadata, restore_owner_defaults,
};
use crate::interchange::export::scope::write_scope;
use crate::interchange::export::xml::{
    di_owner_prefix, esc, metadata_context, write_catalog, write_catalog_qname_attribute,
    write_context_qname_attribute, write_di_attributes, write_di_extension, write_di_qname,
    write_doc, write_expression, write_label, write_structured_documentation,
};
use crate::interchange::import::elements::supported_event_details;
use crate::interchange::import::profile::parse_profile;
use crate::interchange::import::tree::{Elem, tree};
use crate::interchange::shared::PendingBase;
use crate::interchange::shared::{
    MAX_CHOREOGRAPHY_DEPTH, activity_ports, choreography_graph_levels, event_definition_identity,
    event_local_selectors, scopes,
};
use crate::interchange::{ExportFailure, ExportReport, ExportedBpmn, PC_ATTRIBUTES_NS};
use crate::ns::{BPMN, BPMNDI, DC, DI};
use processcraft_core::bpmn_model::{
    EventIo, EventReference, EventResource, Expression, NodeDetails, QNameRef,
};
use processcraft_core::ids::ElementIdentity;
use processcraft_core::ids::ElementSelector;
use processcraft_core::model::*;
use std::collections::BTreeMap;
fn missing_xml_reference(document: &ExportContext) -> Option<ElementSelector> {
    fn check(document: &ExportContext, selector: &ElementSelector) -> Option<ElementSelector> {
        (!document.xml_ids.contains_key(selector)).then(|| selector.clone())
    }
    fn check_ref(document: &ExportContext, reference: &EventReference) -> Option<ElementSelector> {
        match reference {
            EventReference::Local(selector) => check(document, selector),
            EventReference::Qualified(_) => None,
        }
    }
    fn check_key(
        document: &ExportContext,
        key: &processcraft_core::bpmn_activity::CorrelationKey,
    ) -> Option<ElementSelector> {
        key.property_refs
            .iter()
            .find_map(|reference| check_ref(document, reference))
    }
    for property in &document.correlation_properties {
        for retrieval in &property.retrieval_expressions {
            if let Some(missing) = retrieval
                .message_ref
                .as_ref()
                .and_then(|reference| check_ref(document, reference))
            {
                return Some(missing);
            }
        }
    }
    if let Some(collaboration) = &document.collaboration {
        for key in collaboration.correlation_keys.iter().chain(
            collaboration
                .conversation_nodes
                .iter()
                .flat_map(|node| &node.correlation_keys),
        ) {
            if let Some(missing) = check_key(document, key) {
                return Some(missing);
            }
        }
    }
    for choreography in &document.choreographies {
        for key in choreography
            .activities
            .iter()
            .flat_map(|activity| &activity.correlation_keys)
        {
            if let Some(missing) = check_key(document, key) {
                return Some(missing);
            }
        }
    }
    for process in &document.processes {
        if let Some(metadata) = &process.activity_metadata {
            for reference in metadata
                .callable
                .supported_interface_refs
                .iter()
                .chain(&metadata.supports)
                .chain(metadata.definitional_collaboration_ref.iter())
            {
                if let Some(missing) = check_ref(document, reference) {
                    return Some(missing);
                }
            }
            for binding in &metadata.callable.io_bindings {
                if let Some(missing) = binding
                    .operation_ref
                    .as_ref()
                    .and_then(|reference| check_ref(document, reference))
                {
                    return Some(missing);
                }
                for reference in binding
                    .input_data_ref
                    .iter()
                    .chain(binding.output_data_ref.iter())
                {
                    if let Some(missing) = check(document, reference) {
                        return Some(missing);
                    }
                }
            }
            for subscription in &metadata.correlation_subscriptions {
                if let Some(missing) = subscription
                    .key_ref
                    .as_ref()
                    .and_then(|reference| check_ref(document, reference))
                {
                    return Some(missing);
                }
                for binding in &subscription.bindings {
                    if let Some(missing) = binding
                        .property_ref
                        .as_ref()
                        .and_then(|reference| check_ref(document, reference))
                    {
                        return Some(missing);
                    }
                }
            }
        }
    }
    fn io_refs(
        document: &ExportContext,
        io: &processcraft_core::bpmn_data::IoSpecification,
    ) -> Option<ElementSelector> {
        io.input_sets
            .iter()
            .chain(&io.output_sets)
            .flat_map(|set| {
                set.data_refs
                    .iter()
                    .chain(&set.optional_refs)
                    .chain(&set.while_executing_refs)
                    .chain(&set.counterpart_refs)
            })
            .find_map(|r| check(document, r))
    }
    for scope in scopes(document)? {
        for metadata in scope
            .nodes
            .iter()
            .filter_map(|node| node.flow_metadata.as_ref())
            .chain(
                scope
                    .flows
                    .iter()
                    .filter_map(|flow| flow.flow_metadata.as_ref()),
            )
            .chain(
                scope
                    .data_objects
                    .iter()
                    .filter_map(|object| object.flow_metadata.as_ref()),
            )
        {
            for reference in metadata
                .category_value_refs
                .iter()
                .chain(&metadata.incoming)
                .chain(&metadata.outgoing)
            {
                if let Some(missing) = check_ref(document, reference) {
                    return Some(missing);
                }
            }
        }
        for lane in scope.lanes {
            for reference in &lane.members {
                if let Some(missing) = check(document, reference) {
                    return Some(missing);
                }
            }
        }
        for flow in scope.flows {
            for reference in flow.source.iter().chain(flow.target.iter()) {
                if let Some(missing) = check_ref(document, reference) {
                    return Some(missing);
                }
            }
        }
        for group in scope.groups {
            if let Some(reference) = &group.category_value_ref
                && let Some(missing) = check_ref(document, reference)
            {
                return Some(missing);
            }
        }
        for association in scope.data_associations {
            for reference in association
                .sources
                .iter()
                .chain(std::iter::once(&association.target))
            {
                if let Some(missing) = check(document, reference) {
                    return Some(missing);
                }
            }
        }
        for node in scope.nodes {
            if let Some(io) = &node.io_specification
                && let Some(missing) = io_refs(document, io)
            {
                return Some(missing);
            }
            if let Some(default) = node.activity.as_ref().and_then(|a| a.default_flow.as_ref())
                && let Some(missing) = check(document, default)
            {
                return Some(missing);
            }
            let refs: Vec<&ElementSelector> = match &node.details {
                Some(NodeDetails::Gateway { default_flow, .. }) => default_flow.iter().collect(),
                Some(NodeDetails::DataObjectReference { data_object_ref }) => {
                    data_object_ref.iter().collect()
                }
                Some(NodeDetails::DataStoreReference {
                    data_store_ref: Some(processcraft_core::bpmn_data::DataStoreRef::Legacy(r)),
                }) => vec![r],
                Some(NodeDetails::BoundaryEvent {
                    attached_to: EventReference::Local(attached_to),
                    ..
                }) => vec![attached_to],
                Some(NodeDetails::Event {
                    definitions,
                    definition_refs,
                    attached_to,
                    io,
                    ..
                }) => {
                    let mut refs: Vec<_> = attached_to
                        .iter()
                        .filter_map(|r| {
                            if let EventReference::Local(id) = r {
                                Some(id)
                            } else {
                                None
                            }
                        })
                        .collect();
                    for definition in definitions {
                        refs.extend(event_local_selectors(definition));
                    }
                    for reference in definition_refs {
                        if let EventReference::Local(selector) = reference {
                            refs.push(selector);
                        }
                    }
                    let event_set = match io {
                        Some(EventIo::Catch { output_set, .. }) => output_set.as_ref(),
                        Some(EventIo::Throw { input_set, .. }) => input_set.as_ref(),
                        None => None,
                    };
                    if let Some(set) = event_set {
                        refs.extend(set.data_refs.iter());
                        refs.extend(set.optional_refs.iter());
                        refs.extend(set.while_executing_refs.iter());
                        refs.extend(set.counterpart_refs.iter());
                    }
                    refs
                }
                _ => Vec::new(),
            };
            for reference in refs {
                if let Some(missing) = check(document, reference) {
                    return Some(missing);
                }
            }
        }
    }
    for definition in &document.root_event_definitions {
        for reference in event_local_selectors(definition) {
            if let Some(missing) = check(document, reference) {
                return Some(missing);
            }
        }
    }
    for process in &document.processes {
        if let Some(io) = &process.io_specification
            && let Some(missing) = io_refs(document, io)
        {
            return Some(missing);
        }
    }
    if let Some(collab) = &document.collaboration {
        for p in &collab.participants {
            if let Some(r) = &p.process
                && let Some(missing) = check_ref(document, r)
            {
                return Some(missing);
            }
        }
        for f in &collab.flows {
            for r in f.source.iter().chain(f.target.iter()) {
                if let Some(missing) = check_ref(document, r) {
                    return Some(missing);
                }
            }
        }
        for n in &collab.conversation_nodes {
            for r in &n.participant_refs {
                if let Some(missing) = check_ref(document, r) {
                    return Some(missing);
                }
            }
        }
        for l in &collab.conversation_links {
            for r in l.source.iter().chain(l.target.iter()) {
                if let Some(missing) = check_ref(document, r) {
                    return Some(missing);
                }
            }
        }
    }
    for choreography in &document.choreographies {
        for p in &choreography.participants {
            if let Some(r) = &p.process
                && let Some(missing) = check_ref(document, r)
            {
                return Some(missing);
            }
        }
        for f in choreography
            .message_flows
            .iter()
            .chain(&choreography.sequence_flows)
        {
            for r in f.source.iter().chain(f.target.iter()) {
                if let Some(missing) = check_ref(document, r) {
                    return Some(missing);
                }
            }
        }
        for a in &choreography.activities {
            for r in a
                .participant_refs
                .iter()
                .chain(&a.message_flow_refs)
                .chain(a.initiating_participant.iter())
            {
                if let Some(missing) = check_ref(document, r) {
                    return Some(missing);
                }
            }
        }
    }
    for flow in document.collaboration.iter().flat_map(|c| &c.flows).chain(
        document
            .choreographies
            .iter()
            .flat_map(|c| &c.message_flows),
    ) {
        if let Some(missing) = flow
            .message_ref
            .as_ref()
            .and_then(|reference| check_ref(document, reference))
        {
            return Some(missing);
        }
    }
    for diagram in &document.diagrams {
        for shape in &diagram.shapes {
            if let Some(DiStyleRef::Local(reference)) = &shape.choreography_activity_shape
                && let Some(missing) = check(document, reference)
            {
                return Some(missing);
            }
        }
        if let Some(missing) = check(document, &diagram.plane) {
            return Some(missing);
        }
        for edge in &diagram.edges {
            for reference in edge.source_element.iter().chain(edge.target_element.iter()) {
                if let Some(missing) = check(document, reference) {
                    return Some(missing);
                }
            }
        }
        for label in diagram
            .shapes
            .iter()
            .filter_map(|s| s.label.as_ref())
            .chain(diagram.edges.iter().filter_map(|e| e.label.as_ref()))
        {
            if let Some(DiStyleRef::Local(reference)) = &label.style_ref
                && let Some(missing) = check(document, reference)
            {
                return Some(missing);
            }
        }
        for r in diagram
            .shapes
            .iter()
            .map(|s| &s.element)
            .chain(diagram.edges.iter().map(|e| &e.element))
        {
            if let Some(missing) = check(document, r) {
                return Some(missing);
            }
        }
    }
    None
}
pub(in crate::interchange) fn export_document(
    document: &BpmnDocument,
    resources: &[Resource],
    acknowledge_native_fields: bool,
    attributes: Option<&attributes::ExportAttributes>,
) -> Result<ExportedBpmn, ExportFailure> {
    let mut report = ExportReport::default();
    let attributes = attributes.filter(|a| !a.values.is_empty());
    if document.folder.is_some() {
        report
            .omitted
            .push("Document folder is a native-only field".into());
    }
    if let Capability::PreservedSource { source_resource_id } = &document.capability {
        if attributes.is_some() {
            report
                .blocked
                .push("Attribute values cannot be written into preserved source".into());
            return Err(ExportFailure { report });
        }
        if document.folder.is_some() && !acknowledge_native_fields {
            report
                .blocked
                .push("Native-only field omission requires acknowledgement".into());
            return Err(ExportFailure { report });
        }
        if let Some(resource) = resources.iter().find(|r| &r.id == source_resource_id) {
            report.retained.push("Original source bytes".into());
            return Ok(ExportedBpmn {
                bytes: resource.bytes.to_vec(),
                report,
            });
        }
        report
            .blocked
            .push("Original source resource is missing".into());
        return Err(ExportFailure { report });
    }
    let pre = preflight(
        document,
        resources,
        acknowledge_native_fields,
        attributes,
        &mut report,
    )?;
    let original = document;
    let document = pre.injected.as_ref().unwrap_or(original);
    let xml = write_definitions(document, &pre.namespace, &mut report)?;
    verify_round_trip(
        original,
        document,
        attributes,
        pre.owner_metadata,
        &pre.namespace,
        xml,
        report,
    )
}
/// What `preflight` hands to the later phases.
struct Preflight {
    /// The document with `pc:attributes` injected, when attribute values are written.
    injected: Option<BpmnDocument>,
    owner_metadata: BTreeMap<String, PendingBase>,
    namespace: String,
}
/// Records one root element's byte span for root-order restoration.
fn record_root(
    root_spans: &mut crate::root_order_export::RootSpans,
    xml: &str,
    report: &mut ExportReport,
    id: Option<&ElementIdentity>,
    start: usize,
) -> Result<(), ExportFailure> {
    if let Err(reason) = root_spans.record(id, start, xml.len()) {
        report.blocked.push(reason);
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    Ok(())
}
/// Refusals that must happen before anything is written; also resolves the injected
/// attribute document, the owner metadata and the target namespace.
fn preflight(
    document: &BpmnDocument,
    resources: &[Resource],
    acknowledge_native_fields: bool,
    attributes: Option<&attributes::ExportAttributes>,
    report: &mut ExportReport,
) -> Result<Preflight, ExportFailure> {
    if document.diagrams.iter().any(|d| {
        d.plane_metadata
            .as_ref()
            .and_then(|m| m.extension.as_ref())
            .is_some()
            || d.shapes.iter().any(|s| {
                s.di_metadata
                    .as_ref()
                    .and_then(|m| m.extension.as_ref())
                    .is_some()
                    || s.label
                        .as_ref()
                        .and_then(|l| l.di_metadata.as_ref())
                        .and_then(|m| m.extension.as_ref())
                        .is_some()
            })
            || d.edges.iter().any(|e| {
                e.di_metadata
                    .as_ref()
                    .and_then(|m| m.extension.as_ref())
                    .is_some()
                    || e.label
                        .as_ref()
                        .and_then(|l| l.di_metadata.as_ref())
                        .and_then(|m| m.extension.as_ref())
                        .is_some()
            })
    }) {
        report.retained.push(
            "DI extension payloads retained; foreign-schema validation is not verified".into(),
        );
    }
    if document.diagrams.iter().any(|d| d.plane_order.is_none()) {
        report
            .transformed
            .push("Legacy diagram child grouping represented as explicit plane order".into());
    }
    if !matches!(document.capability, Capability::Editable) {
        report
            .blocked
            .push("Retained extensions cannot be safely reconstructed".into());
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    if document
        .extensions
        .iter()
        .any(|extension| extension.must_understand == Some(true))
    {
        report.blocked.push(
            "Required extension semantics are not understood; BPMN export is protected".into(),
        );
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    if document
        .relationships
        .iter()
        .any(|relationship| relationship.sources.is_empty() || relationship.targets.is_empty())
    {
        report.blocked.push(
            "A BPMN relationship requires at least one source and target before export".into(),
        );
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    let mut injected = None;
    let document = if let Some(written) = attributes {
        let mut copy = document.clone();
        if let Err(reason) = attributes::inject(&mut copy, written) {
            report.blocked.push(reason);
            return Err(ExportFailure {
                report: std::mem::take(report),
            });
        }
        report
            .transformed
            .push("Custom attribute values written to the ProcessCraft extension namespace".into());
        injected.insert(copy)
    } else {
        document
    };
    let Ok(owner_metadata) = collect_owner_metadata(document) else {
        report
            .blocked
            .push("Common BPMN metadata is not yet fully representable by this exporter".into());
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    };
    if document
        .diagrams
        .iter()
        .flat_map(|d| &d.label_styles)
        .any(|style| style.font.is_none())
    {
        report
            .blocked
            .push("BPMN label styles require a Font before export".into());
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    let activity_ports = activity_ports(document).unwrap_or_default();
    if document
        .diagrams
        .iter()
        .flat_map(|diagram| &diagram.shapes)
        .any(|shape| activity_ports.contains(&shape.element))
    {
        report.blocked.push("Activity or event data input/output ports cannot be shown in BPMN diagrams; remove their shapes before export".into());
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    if let Some(scopes) = scopes(document) {
        let associations: std::collections::HashMap<_, _> = scopes
            .iter()
            .flat_map(|scope| scope.data_associations)
            .filter_map(|association| {
                association
                    .id
                    .as_ref()
                    .map(|id| (id.selector(), association))
            })
            .collect();
        for edge in document.diagrams.iter().flat_map(|diagram| &diagram.edges) {
            if let Some(association) = associations.get(&edge.element) {
                if association
                    .sources
                    .iter()
                    .any(|source| activity_ports.contains(source))
                    && edge.source_element.is_none()
                {
                    report.blocked.push("Data association uses a non-process output port; select the visible source representation before export".into());
                    return Err(ExportFailure {
                        report: std::mem::take(report),
                    });
                }
                if activity_ports.contains(&association.target) && edge.target_element.is_none() {
                    report.blocked.push("Data association uses a non-process input port; select the visible target representation before export".into());
                    return Err(ExportFailure {
                        report: std::mem::take(report),
                    });
                }
            }
        }
    }
    let context = ExportContext::new(document);
    if let Some(selector) = missing_xml_reference(&context) {
        report.blocked.push(format!(
            "Assign an XML ID to referenced element {selector} before BPMN export"
        ));
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    let Some(all_scopes) = scopes(document) else {
        report
            .blocked
            .push("Unsupported nested scope or depth".into());
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    };
    let native_nodes = all_scopes
        .iter()
        .flat_map(|p| p.nodes)
        .chain(
            document.collaboration.iter().chain(&document.additional_collaborations)
                .filter_map(|c| c.extras.as_ref())
                .chain(document.choreographies.iter().filter_map(|c| c.extras.as_ref()))
                .flat_map(|extras| &extras.artifacts)
                .filter_map(|artifact| match artifact {
                    processcraft_core::bpmn_collaboration::CollaborationArtifact::TextAnnotation(node) => Some(node.as_ref()),
                    _ => None,
                }),
        )
        .any(|n| !n.owner.is_empty() || !n.contact.is_empty())
        || processcraft_core::queries::choreography_scopes(document).iter().any(|scope| {
            scope.events.iter().chain(&scope.gateways).any(|n| !n.owner.is_empty() || !n.contact.is_empty())
                || scope.artifacts.iter().any(|artifact| matches!(artifact, processcraft_core::bpmn_collaboration::CollaborationArtifact::TextAnnotation(node) if !node.owner.is_empty() || !node.contact.is_empty()))
        });
    if native_nodes {
        report
            .omitted
            .push("Node owner and contact are native-only fields".into());
    }
    if (native_nodes || document.folder.is_some()) && !acknowledge_native_fields {
        report
            .blocked
            .push("Native-only field omission requires acknowledgement".into());
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    let conflicting_annotation_name = all_scopes.iter().flat_map(|p| p.nodes).any(|node| {
        matches!((&node.details, &node.name), (Some(NodeDetails::TextAnnotation { text: Some(_), .. }), Some(name)) if !name.is_empty())
    });
    if conflicting_annotation_name {
        report
            .blocked
            .push("Text annotation has a separate native name that cannot be exported".into());
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    // A plain-string condition is text content, but `prefix:name` text whose prefix is
    // bound in the source may be a QName that Legacy text cannot carry a context for.
    let source_prefixes: std::collections::HashSet<String> = document
        .provenance
        .as_ref()
        .and_then(|p| resources.iter().find(|r| r.id == p.source_resource_id))
        .and_then(|r| tree(&r.bytes))
        .map(|root| {
            fn walk(e: &Elem, out: &mut std::collections::HashSet<String>) {
                out.extend(e.namespaces.keys().cloned());
                e.children.iter().for_each(|c| walk(c, out));
            }
            let mut out = std::collections::HashSet::new();
            walk(&root, &mut out);
            out
        })
        .unwrap_or_default();
    let unsupported = all_scopes.iter().any(|p| {
        p.nodes.iter().any(|n| {
            n.io_specification
                .as_ref()
                .is_some_and(|io| io.input_sets.is_empty() || io.output_sets.is_empty())
        }) || p.nodes.iter().any(|n| {
            (n.details.is_some()
                || matches!(
                    n.kind,
                    NodeKind::IntermediateCatchEvent
                        | NodeKind::BoundaryEvent
                        | NodeKind::DataObjectReference
                        | NodeKind::DataStoreReference
                        | NodeKind::SubProcess
                        | NodeKind::CallActivity
                ))
                && !matches!(
                    (&n.kind, &n.details),
                    (
                        NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction,
                        Some(NodeDetails::SubProcess { .. })
                    ) | (
                        NodeKind::DataObjectReference,
                        Some(NodeDetails::DataObjectReference { .. })
                    ) | (
                        NodeKind::DataStoreReference,
                        Some(NodeDetails::DataStoreReference { .. })
                    ) | (
                        NodeKind::CallActivity,
                        Some(NodeDetails::CallActivity { .. })
                    ) | (
                        NodeKind::ExclusiveGateway
                            | NodeKind::InclusiveGateway
                            | NodeKind::ParallelGateway
                            | NodeKind::ComplexGateway
                            | NodeKind::EventBasedGateway,
                        Some(NodeDetails::Gateway { .. })
                    )
                )
                && !matches!(
                    (&n.kind, &n.details),
                    (
                        NodeKind::TextAnnotation,
                        Some(NodeDetails::TextAnnotation { .. })
                    )
                )
                && !supported_event_details(n)
        }) || p.flows.iter().any(|f| {
            !matches!(f.kind, FlowKind::Sequence | FlowKind::Association)
                || (f.kind == FlowKind::Association
                    && (f.name.as_deref().is_some_and(|name| !name.is_empty())
                        || f.condition.is_some()))
                || f.condition.as_ref().is_some_and(|c| {
                    matches!(c, Expression::Legacy(text) if text
                        .split_once(':')
                        .is_some_and(|(prefix, _)| source_prefixes.contains(prefix)))
                })
        })
    }) || document.diagrams.iter().any(|d| {
        d.shapes
            .iter()
            .any(|s| s.style.is_some() || s.label.as_ref().is_some_and(|l| l.style.is_some()))
            || d.edges
                .iter()
                .any(|e| e.style.is_some() || e.label.as_ref().is_some_and(|l| l.style.is_some()))
    });
    if unsupported {
        report.blocked.push(
            "Document contains semantics or diagram details outside the qualified export profile"
                .into(),
        );
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    let namespace = document
        .provenance
        .as_ref()
        .and_then(|p| resources.iter().find(|r| r.id == p.source_resource_id))
        .and_then(|r| tree(&r.bytes))
        .and_then(|e| e.attrs.get("targetNamespace").cloned());
    let namespace = match (
        document
            .definitions
            .as_ref()
            .map(|d| d.target_namespace.clone())
            .or(namespace),
        &document.provenance,
    ) {
        (Some(ns), _) => ns,
        (None, None) => GENERATED_TARGET_NAMESPACE.into(),
        _ => {
            report
                .blocked
                .push("Original target namespace is unavailable".into());
            return Err(ExportFailure {
                report: std::mem::take(report),
            });
        }
    };
    if let Err(reason) = crate::activity_export::validate(document, &namespace) {
        report.blocked.push(reason);
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    Ok(Preflight {
        injected,
        owner_metadata,
        namespace,
    })
}
/// Serializes the `<definitions>` element for the already validated document.
fn write_definitions(
    document: &BpmnDocument,
    namespace: &str,
    report: &mut ExportReport,
) -> Result<String, ExportFailure> {
    let context = ExportContext::new(document);
    // Call activity targets are checked by core reference validation above: a calledElement
    // resolving to a local non-callable element is refused, one naming nothing in this file
    // is an external callable (e.g. an engine process key) and is written verbatim.
    let mut xml =
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?><definitions xmlns=\"{BPMN}\"");
    write_xml_id(&mut xml, &document.definitions_id);
    xml.push_str(&format!(" targetNamespace=\"{}\">", esc(namespace)));
    if let Some(definitions) = &document.definitions {
        // Insert optional metadata before the closing bracket of the root tag.
        xml.pop();
        for (name, value) in [
            ("name", &definitions.name),
            ("expressionLanguage", &definitions.expression_language),
            ("typeLanguage", &definitions.type_language),
            ("exporter", &definitions.exporter),
            ("exporterVersion", &definitions.exporter_version),
        ] {
            if let Some(value) = value {
                xml.push_str(&format!(" {name}=\"{}\"", esc(value)));
            }
        }
        for binding in &definitions.namespaces {
            // Foreign attributes are always prefixed, so the source default namespace
            // (bpmn.io writes its targetNamespace there) is not part of their context.
            if !binding.prefix.is_empty() && binding.prefix != "xml" {
                xml.push_str(&format!(
                    " xmlns:{}=\"{}\"",
                    binding.prefix,
                    esc(&binding.uri)
                ));
            }
        }
        for attribute in &definitions.foreign_attributes {
            let Some(prefix) = &attribute.prefix else {
                report
                    .blocked
                    .push("Definitions foreign attribute lacks a prefix".into());
                return Err(ExportFailure {
                    report: std::mem::take(report),
                });
            };
            xml.push_str(&format!(
                " {prefix}:{}=\"{}\"",
                attribute.local,
                esc(&attribute.value)
            ));
        }
        xml.push('>');
    }
    if let Some(definitions) = &document.definitions {
        for import in &definitions.imports {
            xml.push_str(&format!(
                "<import namespace=\"{}\" location=\"{}\" importType=\"{}\"/>",
                esc(&import.namespace),
                esc(&import.location),
                esc(&import.import_type)
            ));
        }
    }
    for extension in &document.extensions {
        let unqualified = extension
            .definition
            .as_ref()
            .is_some_and(|q| q.namespace.is_empty());
        let tag = if unqualified {
            "pc_definition:extension"
        } else {
            "extension"
        };
        xml.push_str(&format!("<{tag}"));
        if unqualified {
            xml.push_str(&format!(" xmlns:pc_definition=\"{BPMN}\" xmlns=\"\""));
        }
        if let Some(q) = &extension.definition {
            write_catalog_qname_attribute(&mut xml, "definition", q, "pc_extension");
        }
        if let Some(value) = extension.must_understand {
            xml.push_str(&format!(" mustUnderstand=\"{value}\""));
        }
        xml.push('>');
        for documentation in &extension.documentation {
            write_structured_documentation(&mut xml, documentation);
        }
        xml.push_str(&format!("</{tag}>"));
    }
    let mut root_spans = crate::root_order_export::RootSpans::new(document.root_order.is_some());
    write_root_declarations(
        &mut xml,
        document,
        namespace,
        &mut root_spans,
        &context,
        &mut *report,
    )?;
    write_choreographies(
        &mut xml,
        document,
        namespace,
        &mut root_spans,
        &context,
        &mut *report,
    )?;
    write_collaborations(
        &mut xml,
        document,
        namespace,
        &mut root_spans,
        &context,
        &mut *report,
    )?;
    write_processes(
        &mut xml,
        document,
        namespace,
        &mut root_spans,
        &context,
        &mut *report,
    )?;
    if let Err(reason) = root_spans.finish(&mut xml, document.root_order.as_deref()) {
        report.blocked.push(reason);
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    write_diagrams(&mut xml, document, namespace, &context);
    write_relationships(&mut xml, document)?;
    xml.push_str("</definitions>");
    Ok(xml)
}
/// Root-level declarations: catalog, global tasks, categories, data stores, resources, partners and endpoints.
fn write_root_declarations(
    xml: &mut String,
    document: &BpmnDocument,
    namespace: &str,
    root_spans: &mut crate::root_order_export::RootSpans,
    context: &ExportContext,
    report: &mut ExportReport,
) -> Result<(), ExportFailure> {
    if let Err(reason) = write_catalog(xml, &document.catalog, root_spans) {
        report.blocked.push(reason);
        return Err(ExportFailure {
            report: std::mem::take(report),
        });
    }
    for task in &document.global_tasks {
        let start = xml.len();
        write_global_task(xml, task, namespace, context);
        record_root(root_spans, xml, report, task.id.as_ref(), start)?;
    }
    for property in &document.correlation_properties {
        let start = xml.len();
        write_correlation_property(xml, property, namespace, context);
        record_root(root_spans, xml, report, property.id.as_ref(), start)?;
    }
    for category in &document.categories {
        let start = xml.len();
        xml.push_str("<category");
        write_xml_id(xml, &category.id);
        if let Some(name) = &category.name {
            xml.push_str(&format!(" name=\"{}\"", esc(name)));
        }
        anonymous_base_attributes(xml, Some(&category.id), category.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, Some(&category.id), category.base.as_ref());
        for value in &category.values {
            xml.push_str("<categoryValue");
            write_xml_id(xml, &value.id);
            if let Some(text) = &value.value {
                xml.push_str(&format!(" value=\"{}\"", esc(text)));
            }
            finish_anonymous_base_empty(xml, "categoryValue", Some(&value.id), value.base.as_ref());
        }
        xml.push_str("</category>");
        record_root(root_spans, xml, report, Some(&category.id), start)?;
    }
    for store in &document.data_stores {
        let start = xml.len();
        let unqualified = store
            .item_aware
            .as_ref()
            .and_then(|m| m.item_subject_ref.as_ref())
            .is_some_and(|q| q.namespace.is_empty());
        let tag = if unqualified {
            "pc_data:dataStore"
        } else {
            "dataStore"
        };
        xml.push_str(&format!("<{tag}"));
        if unqualified {
            xml.push_str(&format!(" xmlns:pc_data=\"{BPMN}\" xmlns=\"\""));
        }
        if let Some(id) = &store.id {
            write_xml_id(xml, id);
        }
        if let Some(name) = &store.name {
            xml.push_str(&format!(" name=\"{}\"", esc(name)));
        }
        write_item_attributes(
            xml,
            store.item_aware.as_ref(),
            metadata_context(store.base.as_ref()),
        );
        if let Some(capacity) = &store.capacity {
            xml.push_str(&format!(" capacity=\"{}\"", esc(capacity)));
        }
        if let Some(unlimited) = store.is_unlimited {
            xml.push_str(&format!(" isUnlimited=\"{}\"", unlimited));
        }
        anonymous_base_attributes(xml, store.id.as_ref(), store.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, store.id.as_ref(), store.base.as_ref());
        write_item_state(xml, store.item_aware.as_ref(), unqualified);
        xml.push_str(&format!("</{tag}>"));
        record_root(root_spans, xml, report, store.id.as_ref(), start)?;
    }
    for resource in &document.event_resources {
        let start = xml.len();
        let (tag, id, name, qname) = match resource {
            EventResource::Signal {
                id,
                name,
                structure_ref,
                ..
            } => (
                "signal",
                id,
                name,
                structure_ref.as_ref().map(|q| ("structureRef", q)),
            ),
            EventResource::Message {
                id, name, item_ref, ..
            } => (
                "message",
                id,
                name,
                item_ref.as_ref().map(|q| ("itemRef", q)),
            ),
            EventResource::Error {
                id,
                name,
                structure_ref,
                ..
            } => (
                "error",
                id,
                name,
                structure_ref.as_ref().map(|q| ("structureRef", q)),
            ),
            EventResource::Escalation {
                id,
                name,
                structure_ref,
                ..
            } => (
                "escalation",
                id,
                name,
                structure_ref.as_ref().map(|q| ("structureRef", q)),
            ),
        };
        let owner_tag = if let Some((_, q)) = qname {
            write_local_qname_open(xml, tag, &q.namespace)
        } else {
            xml.push_str(&format!("<{tag}"));
            tag.to_owned()
        };
        write_xml_id(xml, id);
        write_xml_name(xml, name.as_deref());
        if let Some((attribute, q)) = qname {
            xml.push_str(&format!(
                " {attribute}=\"{}\"",
                local_qname_value(&q.local, &q.namespace)
            ));
        }
        match resource {
            EventResource::Error {
                code: Some(code), ..
            } => xml.push_str(&format!(" errorCode=\"{}\"", esc(code))),
            EventResource::Escalation {
                code: Some(code), ..
            } => xml.push_str(&format!(" escalationCode=\"{}\"", esc(code))),
            _ => {}
        }
        finish_anonymous_base_empty(xml, &owner_tag, Some(id), resource.base());
        record_root(root_spans, xml, report, Some(id), start)?;
    }
    for definition in &document.root_event_definitions {
        let start = xml.len();
        write_event_definition(xml, definition, false, namespace, context);
        record_root(
            root_spans,
            xml,
            report,
            event_definition_identity(definition),
            start,
        )?;
    }
    for value in &document.partner_entities {
        let start = xml.len();
        xml.push_str("<partnerEntity");
        if let Some(id) = &value.id {
            write_xml_id(xml, id);
        }
        write_xml_name(xml, value.name.as_deref());
        anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
        for reference in &value.participant_refs {
            write_event_qname_text(xml, "participantRef", reference, namespace, context);
        }
        xml.push_str("</partnerEntity>");
        record_root(root_spans, xml, report, value.id.as_ref(), start)?;
    }
    for value in &document.partner_roles {
        let start = xml.len();
        xml.push_str("<partnerRole");
        if let Some(id) = &value.id {
            write_xml_id(xml, id);
        }
        write_xml_name(xml, value.name.as_deref());
        anonymous_base_attributes(xml, value.id.as_ref(), value.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, value.id.as_ref(), value.base.as_ref());
        for reference in &value.participant_refs {
            write_event_qname_text(xml, "participantRef", reference, namespace, context);
        }
        xml.push_str("</partnerRole>");
        record_root(root_spans, xml, report, value.id.as_ref(), start)?;
    }
    for value in &document.endpoints {
        let start = xml.len();
        xml.push_str("<endPoint");
        if let Some(id) = &value.id {
            write_xml_id(xml, id);
        }
        finish_anonymous_base_empty(xml, "endPoint", value.id.as_ref(), value.base.as_ref());
        record_root(root_spans, xml, report, value.id.as_ref(), start)?;
    }
    Ok(())
}
/// Choreography roots.
fn write_choreographies(
    xml: &mut String,
    document: &BpmnDocument,
    namespace: &str,
    root_spans: &mut crate::root_order_export::RootSpans,
    context: &ExportContext,
    report: &mut ExportReport,
) -> Result<(), ExportFailure> {
    for c in &document.choreographies {
        let start = xml.len();
        if c.kind.is_some()
            && (c
                .graph
                .as_ref()
                .is_some_and(|g| !g.flow_elements.is_empty())
                || !c.activities.is_empty()
                || !c.sequence_flows.is_empty())
        {
            report.blocked.push(
                "BPMN 2.0.2 §11.5.4 forbids Flow Elements in a Global Choreography Task".into(),
            );
            return Err(ExportFailure {
                report: std::mem::take(report),
            });
        }
        if c.graph.is_some() && (!c.activities.is_empty() || !c.sequence_flows.is_empty()) {
            report
                .blocked
                .push("Choreography contains both graph and legacy flow elements".into());
            return Err(ExportFailure {
                report: std::mem::take(report),
            });
        }
        if c.activities
            .iter()
            .any(|a| a.participant_refs.len() < 2 || !(1..=2).contains(&a.message_flow_refs.len()))
        {
            report.blocked.push("Incomplete choreography task".into());
            return Err(ExportFailure {
                report: std::mem::take(report),
            });
        }
        let root_tag = match &c.kind {
            None => {
                xml.push_str("<choreography");
                "choreography".to_owned()
            }
            Some(processcraft_core::bpmn_collaboration::ChoreographyRootKind::Global {
                initiating_participant_ref,
            }) => {
                let refs: Vec<_> = initiating_participant_ref
                    .iter()
                    .map(|r| ("initiatingParticipantRef", r))
                    .collect();
                let mut scopes = Vec::new();
                collaboration_base_scopes(c.base.as_ref(), &mut scopes);
                write_collaboration_qname_owner(
                    xml,
                    "globalChoreographyTask",
                    &refs,
                    namespace,
                    context,
                    &scopes,
                )
            }
        };
        write_xml_id(xml, &c.id);
        write_xml_name(xml, c.name.as_deref());
        if let Some(value) = c.extras.as_ref().and_then(|e| e.is_closed) {
            xml.push_str(&format!(" isClosed=\"{value}\""));
        }
        anonymous_base_attributes(xml, Some(&c.id), c.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, Some(&c.id), c.base.as_ref());
        for p in &c.participants {
            write_participant(xml, p, namespace, context);
        }
        for f in &c.message_flows {
            write_message_flow(xml, f, namespace, context);
        }
        write_collaboration_artifacts(xml, c.extras.as_ref(), namespace, context);
        write_conversations(xml, &c.conversation_nodes, None, namespace, context);
        write_collaboration_associations(xml, c.extras.as_ref(), namespace, context);
        for key in &c.correlation_keys {
            write_correlation_key(xml, key, namespace, context);
        }
        write_collaboration_refs(xml, c.extras.as_ref(), namespace, context);
        write_conversation_links(xml, &c.conversation_links, namespace, context);
        if let Some(graph) = &c.graph
            && choreography_graph_levels(graph) > MAX_CHOREOGRAPHY_DEPTH + 1
        {
            report.blocked.push(format!(
                "Choreography nesting depth exceeds {MAX_CHOREOGRAPHY_DEPTH} levels"
            ));
            return Err(ExportFailure {
                report: std::mem::take(report),
            });
        }
        if let Some(graph) = &c.graph
            && !write_choreography_graph(xml, graph, namespace, context, 0)
        {
            report
                .blocked
                .push("Incomplete or unsupported choreography graph".into());
            return Err(ExportFailure {
                report: std::mem::take(report),
            });
        }
        for a in &c.activities {
            if a.details.is_some() || a.initiating_participant.is_none() {
                report
                    .blocked
                    .push("Choreography activity cannot be represented by this XML writer".into());
                return Err(ExportFailure {
                    report: std::mem::take(report),
                });
            }
            let refs = [(
                "initiatingParticipantRef",
                a.initiating_participant.as_ref().expect("checked"),
            )];
            let mut scopes = Vec::new();
            collaboration_base_scopes(a.base.as_ref(), &mut scopes);
            for key in &a.correlation_keys {
                collaboration_base_scopes(key.base.as_ref(), &mut scopes);
            }
            let tag = write_collaboration_qname_owner(
                xml,
                "choreographyTask",
                &refs,
                namespace,
                context,
                &scopes,
            );
            write_xml_id(xml, &a.id);
            write_xml_name(xml, a.name.as_deref());
            if let Some(loop_type) = a.loop_type {
                use processcraft_core::bpmn_collaboration::ChoreographyLoopType;
                let value = match loop_type {
                    ChoreographyLoopType::None => "None",
                    ChoreographyLoopType::Standard => "Standard",
                    ChoreographyLoopType::MultiInstanceSequential => "MultiInstanceSequential",
                    ChoreographyLoopType::MultiInstanceParallel => "MultiInstanceParallel",
                };
                xml.push_str(&format!(" loopType=\"{value}\""));
            }
            anonymous_base_attributes(xml, Some(&a.id), a.base.as_ref());
            xml.push('>');
            anonymous_base_children(xml, Some(&a.id), a.base.as_ref());
            write_flow_metadata(xml, a.flow_metadata.as_ref(), namespace, context);
            for participant in &a.participant_refs {
                write_event_qname_text(xml, "participantRef", participant, namespace, context);
            }
            for key in &a.correlation_keys {
                write_correlation_key(xml, key, namespace, context);
            }
            for flow in &a.message_flow_refs {
                write_event_qname_text(xml, "messageFlowRef", flow, namespace, context);
            }
            xml.push_str(&format!("</{tag}>"));
        }
        for f in &c.sequence_flows {
            xml.push_str("<sequenceFlow");
            write_xml_id(xml, &f.id);
            write_xml_name(xml, f.name.as_deref());
            xml.push_str(&format!(
                " sourceRef=\"{}\" targetRef=\"{}\"",
                local_flow_xml_reference(context, f.source.as_ref()),
                local_flow_xml_reference(context, f.target.as_ref())
            ));
            if let Some(value) = f.flow_metadata.as_ref().and_then(|m| m.is_immediate) {
                xml.push_str(&format!(" isImmediate=\"{value}\""));
            }
            anonymous_extras_attributes(xml, &f.id, f.base_extras.as_ref());
            xml.push('>');
            write_doc(xml, &f.documentation, f.base_extras.as_ref());
            write_flow_metadata(xml, f.flow_metadata.as_ref(), namespace, context);
            if let Some(condition) = &f.condition {
                write_expression(xml, "conditionExpression", condition, true);
            }
            xml.push_str("</sequenceFlow>");
        }
        xml.push_str(&format!("</{root_tag}>"));
        record_root(root_spans, xml, report, Some(&c.id), start)?;
    }
    Ok(())
}
/// Collaboration roots.
fn write_collaborations(
    xml: &mut String,
    document: &BpmnDocument,
    namespace: &str,
    root_spans: &mut crate::root_order_export::RootSpans,
    context: &ExportContext,
    report: &mut ExportReport,
) -> Result<(), ExportFailure> {
    for c in document
        .collaboration
        .iter()
        .chain(&document.additional_collaborations)
    {
        let start = xml.len();
        let tag = if c.kind.is_some() {
            "globalConversation"
        } else {
            "collaboration"
        };
        xml.push_str(&format!("<{tag}"));
        write_xml_id(xml, &c.id);
        write_xml_name(xml, c.name.as_deref());
        if let Some(value) = c.extras.as_ref().and_then(|e| e.is_closed) {
            xml.push_str(&format!(" isClosed=\"{value}\""));
        }
        anonymous_base_attributes(xml, Some(&c.id), c.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, Some(&c.id), c.base.as_ref());
        for p in &c.participants {
            write_participant(xml, p, namespace, context);
        }
        for f in &c.flows {
            write_message_flow(xml, f, namespace, context);
        }
        write_collaboration_artifacts(xml, c.extras.as_ref(), namespace, context);
        write_conversations(xml, &c.conversation_nodes, None, namespace, context);
        write_collaboration_associations(xml, c.extras.as_ref(), namespace, context);
        for key in &c.correlation_keys {
            write_correlation_key(xml, key, namespace, context);
        }
        write_collaboration_refs(xml, c.extras.as_ref(), namespace, context);
        write_conversation_links(xml, &c.conversation_links, namespace, context);
        xml.push_str(&format!("</{tag}>"));
        record_root(root_spans, xml, report, Some(&c.id), start)?;
    }
    Ok(())
}
/// Process roots.
fn write_processes(
    xml: &mut String,
    document: &BpmnDocument,
    namespace: &str,
    root_spans: &mut crate::root_order_export::RootSpans,
    context: &ExportContext,
    report: &mut ExportReport,
) -> Result<(), ExportFailure> {
    for p in &document.processes {
        let start = xml.len();
        xml.push_str("<process");
        write_xml_id(xml, &p.id);
        write_xml_name(xml, p.name.as_deref());
        if let Some(value) = p.is_executable {
            xml.push_str(&format!(" isExecutable=\"{value}\""));
        }
        if let Some(metadata) = &p.activity_metadata {
            if let Some(value) = metadata.process_type {
                xml.push_str(&format!(" processType=\"{value:?}\""));
            }
            if let Some(value) = metadata.is_closed {
                xml.push_str(&format!(" isClosed=\"{value}\""));
            }
            if let Some(reference) = &metadata.definitional_collaboration_ref {
                let q = event_reference_qname(reference, namespace, context);
                if let Some(extra) = p.base_extras.as_ref().filter(|e| !e.namespaces.is_empty()) {
                    write_context_qname_attribute(
                        xml,
                        "definitionalCollaborationRef",
                        &q,
                        &extra.namespaces,
                    );
                } else {
                    write_catalog_qname_attribute(
                        xml,
                        "definitionalCollaborationRef",
                        &q,
                        "pc_collab",
                    );
                }
            }
        }
        anonymous_extras_attributes(xml, &p.id, p.base_extras.as_ref());
        xml.push('>');
        write_doc(xml, &p.documentation, p.base_extras.as_ref());
        if let Some(metadata) = &p.activity_metadata {
            for reference in &metadata.callable.supported_interface_refs {
                write_event_qname_text(xml, "supportedInterfaceRef", reference, namespace, context);
            }
        }
        if let Some(io) = &p.io_specification {
            write_io_specification(xml, io, false, context);
        }
        if let Some(metadata) = &p.activity_metadata {
            for binding in &metadata.callable.io_bindings {
                write_io_binding(xml, binding, namespace, context);
            }
            if let Some(audit) = &metadata.auditing {
                write_audit_monitor(xml, "auditing", audit);
            }
            if let Some(monitor) = &metadata.monitoring {
                write_audit_monitor(xml, "monitoring", monitor);
            }
            for property in &metadata.properties {
                write_event_property(xml, property, false);
            }
        }
        if p.lane_set_id.is_some()
            || p.lane_set_base.is_some()
            || p.lane_set_name.is_some()
            || !p.lanes.is_empty()
        {
            xml.push_str("<laneSet");
            if let Some(id) = &p.lane_set_id {
                write_xml_id(xml, id);
            }
            write_xml_name(xml, p.lane_set_name.as_deref());
            anonymous_base_attributes(xml, p.lane_set_id.as_ref(), p.lane_set_base.as_ref());
            xml.push('>');
            anonymous_base_children(xml, p.lane_set_id.as_ref(), p.lane_set_base.as_ref());
            write_lanes(xml, &p.lanes, None, context, namespace);
            xml.push_str("</laneSet>");
        }
        write_scope(
            xml,
            &p.nodes,
            &p.flows,
            &p.data_objects,
            &p.data_associations,
            &p.groups,
            context,
            namespace,
        );
        if let Some(metadata) = &p.activity_metadata {
            for role in &metadata.resource_roles {
                write_resource_role(xml, role, namespace, context);
            }
            for subscription in &metadata.correlation_subscriptions {
                write_correlation_subscription(xml, subscription, namespace, context);
            }
            for reference in &metadata.supports {
                write_event_qname_text(xml, "supports", reference, namespace, context);
            }
        }
        xml.push_str("</process>");
        record_root(root_spans, xml, report, Some(&p.id), start)?;
    }
    Ok(())
}
/// BPMN diagrams (DI).
fn write_diagrams(
    xml: &mut String,
    document: &BpmnDocument,
    namespace: &str,
    context: &ExportContext,
) {
    for d in &document.diagrams {
        let ns_binding = format!(" xmlns=\"{}\"", esc(namespace));
        let metadata_contexts: Vec<_> = d
            .plane_metadata
            .iter()
            .chain(d.shapes.iter().filter_map(|s| s.di_metadata.as_ref()))
            .chain(d.edges.iter().filter_map(|e| e.di_metadata.as_ref()))
            .chain(
                d.shapes
                    .iter()
                    .filter_map(|s| s.label.as_ref().and_then(|l| l.di_metadata.as_ref())),
            )
            .chain(
                d.edges
                    .iter()
                    .filter_map(|e| e.label.as_ref().and_then(|l| l.di_metadata.as_ref())),
            )
            .collect();
        let di_prefix = metadata_contexts
            .first()
            .and_then(|m| {
                m.namespaces.iter().find(|n| {
                    !n.prefix.is_empty()
                        && n.prefix != "xml"
                        && metadata_contexts.iter().all(|other| {
                            other
                                .namespaces
                                .iter()
                                .any(|candidate| candidate.prefix == n.prefix)
                        })
                })
            })
            .map(|n| n.prefix.as_str())
            .unwrap_or("bpmndi");
        xml.push_str(&format!(
            "<{di_prefix}:BPMNDiagram xmlns:{di_prefix}=\"{BPMNDI}\""
        ));
        write_xml_id(xml, &d.id);
        write_xml_name(xml, d.name.as_deref());
        if let Some(documentation) = &d.documentation {
            xml.push_str(&format!(" documentation=\"{}\"", esc(documentation)));
        }
        if let Some(resolution) = d.resolution {
            xml.push_str(&format!(" resolution=\"{resolution}\""));
        }
        let plane_prefix = di_owner_prefix(d.plane_metadata.as_ref(), di_prefix);
        xml.push_str(&format!("><{plane_prefix}:BPMNPlane"));
        if d.plane_metadata.is_some() {
            write_di_attributes(xml, d.plane_metadata.as_ref());
        } else {
            xml.push_str(&ns_binding);
        }
        write_xml_id(xml, &d.plane_id);
        write_di_qname(
            xml,
            "bpmnElement",
            namespace,
            xml_reference(context, &d.plane),
            d.plane_metadata.as_ref(),
        );
        xml.push('>');
        write_di_extension(xml, d.plane_metadata.as_ref());
        let order: Vec<_> = d.plane_order.clone().unwrap_or_else(|| {
            d.shapes
                .iter()
                .map(|s| s.id.selector())
                .chain(d.edges.iter().map(|e| e.id.selector()))
                .collect()
        });
        let shapes: std::collections::HashMap<_, _> = d
            .shapes
            .iter()
            .map(|shape| (shape.id.selector(), shape))
            .collect();
        let edges: std::collections::HashMap<_, _> = d
            .edges
            .iter()
            .map(|edge| (edge.id.selector(), edge))
            .collect();
        for selector in order {
            if let Some(s) = shapes.get(&selector) {
                let b = &s.bounds;
                let shape_prefix = di_owner_prefix(s.di_metadata.as_ref(), plane_prefix);
                xml.push_str(&format!("<{shape_prefix}:BPMNShape"));
                let empty_band_namespace = s.di_metadata.is_none()
                    && !namespace.is_empty()
                    && matches!(&s.choreography_activity_shape, Some(DiStyleRef::Qualified(q)) if q.namespace.is_empty());
                let semantic_prefix = s
                    .label
                    .as_ref()
                    .and_then(|label| label.di_metadata.as_ref())
                    .and_then(|metadata| {
                        metadata
                            .namespaces
                            .iter()
                            .find(|binding| {
                                !binding.prefix.is_empty()
                                    && binding.prefix != shape_prefix
                                    && binding.uri == namespace
                            })
                            .or_else(|| {
                                metadata.namespaces.iter().find(|binding| {
                                    !binding.prefix.is_empty()
                                        && binding.prefix != "xml"
                                        && binding.prefix != shape_prefix
                                })
                            })
                    })
                    .map(|binding| binding.prefix.as_str())
                    .unwrap_or("pc_semantic");
                if s.di_metadata.is_some() {
                    write_di_attributes(xml, s.di_metadata.as_ref());
                } else if empty_band_namespace {
                    xml.push_str(&format!(
                        " xmlns=\"\" xmlns:{semantic_prefix}=\"{}\"",
                        esc(namespace)
                    ));
                } else {
                    xml.push_str(&ns_binding);
                }
                write_xml_id(xml, &s.id);
                if empty_band_namespace {
                    xml.push_str(&format!(
                        " bpmnElement=\"{semantic_prefix}:{}\"",
                        xml_reference(context, &s.element)
                    ));
                } else {
                    write_di_qname(
                        xml,
                        "bpmnElement",
                        namespace,
                        xml_reference(context, &s.element),
                        s.di_metadata.as_ref(),
                    );
                }
                if let Some(expanded) = s.is_expanded {
                    xml.push_str(&format!(" isExpanded=\"{}\"", expanded));
                }
                if let Some(horizontal) = s.is_horizontal {
                    xml.push_str(&format!(" isHorizontal=\"{}\"", horizontal));
                }
                if let Some(marker) = s.is_marker_visible {
                    xml.push_str(&format!(" isMarkerVisible=\"{}\"", marker));
                }
                if let Some(value) = s.is_message_visible {
                    xml.push_str(&format!(" isMessageVisible=\"{value}\""));
                }
                if let Some(value) = s.participant_band_kind {
                    let token = match value {
                        ParticipantBandKind::TopInitiating => "top_initiating",
                        ParticipantBandKind::MiddleInitiating => "middle_initiating",
                        ParticipantBandKind::BottomInitiating => "bottom_initiating",
                        ParticipantBandKind::TopNonInitiating => "top_non_initiating",
                        ParticipantBandKind::MiddleNonInitiating => "middle_non_initiating",
                        ParticipantBandKind::BottomNonInitiating => "bottom_non_initiating",
                    };
                    xml.push_str(&format!(" participantBandKind=\"{token}\""));
                }
                if let Some(reference) = &s.choreography_activity_shape {
                    let q = match reference {
                        DiStyleRef::Local(id) => QNameRef {
                            namespace: namespace.to_string(),
                            local: xml_reference(context, id).clone(),
                        },
                        DiStyleRef::Qualified(q) => q.clone(),
                    };
                    if s.di_metadata.is_some() {
                        write_di_qname(
                            xml,
                            "choreographyActivityShape",
                            &q.namespace,
                            &q.local,
                            s.di_metadata.as_ref(),
                        );
                    } else if q.namespace == namespace
                        || (empty_band_namespace && q.namespace.is_empty())
                    {
                        xml.push_str(&format!(" choreographyActivityShape=\"{}\"", q.local));
                    } else {
                        xml.push_str(&format!(" xmlns:pc_band_ref=\"{}\" choreographyActivityShape=\"pc_band_ref:{}\"", esc(&q.namespace), q.local));
                    }
                }
                xml.push('>');
                write_di_extension(xml, s.di_metadata.as_ref());
                xml.push_str(&format!(
                    "<dc:Bounds xmlns:dc=\"{DC}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
                    b.x, b.y, b.width, b.height
                ));
                if let Some(label) = &s.label {
                    write_label(xml, label, namespace, context, shape_prefix);
                } else if s.label_visibility.is_none() {
                    xml.push_str(&format!("<{shape_prefix}:BPMNLabel/>"));
                }
                xml.push_str(&format!("</{shape_prefix}:BPMNShape>"));
            } else if let Some(e) = edges.get(&selector) {
                let edge_prefix = di_owner_prefix(e.di_metadata.as_ref(), plane_prefix);
                xml.push_str(&format!("<{edge_prefix}:BPMNEdge"));
                if e.di_metadata.is_some() {
                    write_di_attributes(xml, e.di_metadata.as_ref());
                } else {
                    xml.push_str(&ns_binding);
                }
                write_xml_id(xml, &e.id);
                write_di_qname(
                    xml,
                    "bpmnElement",
                    namespace,
                    xml_reference(context, &e.element),
                    e.di_metadata.as_ref(),
                );
                if let Some(source) = &e.source_element {
                    write_di_qname(
                        xml,
                        "sourceElement",
                        namespace,
                        xml_reference(context, source),
                        e.di_metadata.as_ref(),
                    );
                }
                if let Some(target) = &e.target_element {
                    write_di_qname(
                        xml,
                        "targetElement",
                        namespace,
                        xml_reference(context, target),
                        e.di_metadata.as_ref(),
                    );
                }
                if let Some(value) = e.message_visible_kind {
                    let token = match value {
                        MessageVisibleKind::Initiating => "initiating",
                        MessageVisibleKind::NonInitiating => "non_initiating",
                    };
                    xml.push_str(&format!(" messageVisibleKind=\"{token}\""));
                }
                xml.push('>');
                write_di_extension(xml, e.di_metadata.as_ref());
                for p in &e.waypoints {
                    xml.push_str(&format!(
                        "<di:waypoint xmlns:di=\"{DI}\" x=\"{}\" y=\"{}\"/>",
                        p.x, p.y
                    ));
                }
                if let Some(label) = &e.label {
                    write_label(xml, label, namespace, context, edge_prefix);
                } else if e.label_visibility.is_none() {
                    xml.push_str(&format!("<{edge_prefix}:BPMNLabel/>"));
                }
                xml.push_str(&format!("</{edge_prefix}:BPMNEdge>"));
            }
        }
        xml.push_str(&format!("</{plane_prefix}:BPMNPlane>"));
        for style in &d.label_styles {
            xml.push_str(&format!("<{di_prefix}:BPMNLabelStyle"));
            if let Some(id) = &style.id {
                write_xml_id(xml, id);
            }
            xml.push('>');
            if let Some(font) = &style.font {
                xml.push_str(&format!("<dc:Font xmlns:dc=\"{DC}\""));
                if let Some(name) = &font.name {
                    xml.push_str(&format!(" name=\"{}\"", esc(name)));
                }
                if let Some(size) = font.size {
                    xml.push_str(&format!(" size=\"{size}\""));
                }
                for (key, value) in [
                    ("isBold", font.is_bold),
                    ("isItalic", font.is_italic),
                    ("isUnderline", font.is_underline),
                    ("isStrikeThrough", font.is_strike_through),
                ] {
                    if let Some(value) = value {
                        xml.push_str(&format!(" {key}=\"{value}\""));
                    }
                }
                xml.push_str("/>");
            }
            xml.push_str(&format!("</{di_prefix}:BPMNLabelStyle>"));
        }
        xml.push_str(&format!("</{di_prefix}:BPMNDiagram>"));
    }
}
/// BPMN relationship elements.
fn write_relationships(xml: &mut String, document: &BpmnDocument) -> Result<(), ExportFailure> {
    for relationship in &document.relationships {
        xml.push_str("<relationship");
        if let Some(id) = &relationship.id {
            write_xml_id(xml, id);
        }
        xml.push_str(&format!(
            " type=\"{}\"",
            esc(&relationship.relationship_type)
        ));
        if let Some(direction) = relationship.direction {
            xml.push_str(&format!(" direction=\"{direction:?}\""));
        }
        anonymous_base_attributes(xml, relationship.id.as_ref(), relationship.base.as_ref());
        xml.push('>');
        anonymous_base_children(xml, relationship.id.as_ref(), relationship.base.as_ref());
        for (tag, values) in [
            ("source", &relationship.sources),
            ("target", &relationship.targets),
        ] {
            for q in values {
                xml.push_str(&format!("<pc_relationship:{tag} xmlns:pc_relationship=\"{BPMN}\" xmlns=\"{}\">{}</pc_relationship:{tag}>", esc(&q.namespace), q.local));
            }
        }
        xml.push_str("</relationship>");
    }
    Ok(())
}
/// Re-reads the written XML through the qualified profile and compares it with the source model.
fn verify_round_trip(
    original: &BpmnDocument,
    document: &BpmnDocument,
    attributes: Option<&attributes::ExportAttributes>,
    owner_metadata: BTreeMap<String, PendingBase>,
    namespace: &str,
    xml: String,
    mut report: ExportReport,
) -> Result<ExportedBpmn, ExportFailure> {
    let Some(xml) = restore_owner_defaults(&xml, &owner_metadata) else {
        report
            .blocked
            .push("BPMN owner namespace context cannot be restored safely".into());
        return Err(ExportFailure { report });
    };
    let Some(xml) = insert_owner_metadata(&xml, owner_metadata) else {
        report
            .blocked
            .push("BPMN metadata owner or namespace context cannot be represented safely".into());
        return Err(ExportFailure { report });
    };
    let reparsed = parse_profile(xml.as_bytes());
    if let Some(parsed) = &reparsed {
        // With attribute values, both sides are compared without `pc:attributes`
        // and the values read back must equal the values written.
        // The same applies when the document keeps `pc:attributes` it could not place.
        let none = attributes::ExportAttributes {
            definitions: &[],
            entries: &[],
            values: Vec::new(),
        };
        let normalized = (attributes.is_some() || xml.contains(PC_ATTRIBUTES_NS)).then(|| {
            let written = attributes.unwrap_or(&none);
            let mut expected = original.clone();
            let kept = attributes::lift(&mut expected, &mut Vec::new());
            let mut actual = parsed.clone();
            let read = attributes::lift(&mut actual, &mut Vec::new());
            (
                expected,
                actual,
                attributes::matches_written(&read, &kept, written),
            )
        });
        if normalized.as_ref().is_some_and(|(_, _, same)| !same) {
            report.blocked.push(
                "Attribute values do not survive the BPMN round trip; they are not written".into(),
            );
            return Err(ExportFailure { report });
        }
        let (expected_document, actual_document) = match &normalized {
            Some((expected, actual, _)) => (expected, actual),
            None => (document, parsed),
        };
        match (
            crate::interchange_compare::xml_model_value_for_export(expected_document, namespace),
            crate::interchange_compare::xml_model_value_for_export(actual_document, namespace),
        ) {
            (Ok(expected), Ok(actual)) if expected == actual => {}
            (Ok(expected), Ok(actual)) => {
                report.blocked.push(format!(
                    "BPMN typed owner paths or XML-representable fields cannot be preserved (first difference at {})",
                    first_difference(&expected, &actual, String::new())
                ));
                return Err(ExportFailure { report });
            }
            (expected, actual) => {
                let error = expected.err().or(actual.err()).unwrap_or_default();
                report.blocked.push(format!(
                    "BPMN typed owner paths or XML-representable fields cannot be preserved ({error})"
                ));
                return Err(ExportFailure { report });
            }
        }
    }
    if import_bpmn(xml.as_bytes()).is_err() || reparsed.is_none() {
        report
            .blocked
            .push("Document cannot be represented by the qualified profile".into());
        return Err(ExportFailure { report });
    }
    report
        .retained
        .push("Qualified semantics and diagram geometry".into());
    report
        .transformed
        .push("Canonical XML serialization".into());
    if document.diagrams.iter().any(|diagram| {
        diagram
            .shapes
            .iter()
            .any(|shape| shape.label_visibility.is_none())
            || diagram
                .edges
                .iter()
                .any(|edge| edge.label_visibility.is_none())
    }) {
        report
            .transformed
            .push("Legacy implicit diagram labels represented as explicit BPMN labels".into());
    }
    Ok(ExportedBpmn {
        bytes: xml.into_bytes(),
        report,
    })
}
/// Path of the first place two normalized model values differ, for diagnostics.
fn first_difference(a: &serde_json::Value, b: &serde_json::Value, path: String) -> String {
    use serde_json::Value::{Array, Object};
    let at = |p: &str| {
        if p.is_empty() {
            "/".to_string()
        } else {
            p.to_string()
        }
    };
    match (a, b) {
        (Object(x), Object(y)) => {
            for (k, v) in x {
                let p = format!("{path}/{k}");
                match y.get(k) {
                    Some(w) if w == v => {}
                    Some(w) => return first_difference(v, w, p),
                    None => return format!("{p} (missing after re-import)"),
                }
            }
            match y.keys().find(|k| !x.contains_key(*k)) {
                Some(k) => format!("{path}/{k} (added by re-import)"),
                None => at(&path),
            }
        }
        (Array(x), Array(y)) => {
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                if v != w {
                    return first_difference(v, w, format!("{path}/{i}"));
                }
            }
            format!("{} (length {} vs {})", at(&path), x.len(), y.len())
        }
        _ => format!(
            "{}: {} vs {}",
            at(&path),
            a.to_string().chars().take(120).collect::<String>(),
            b.to_string().chars().take(120).collect::<String>()
        ),
    }
}
