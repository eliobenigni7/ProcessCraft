//! Per-document structural checks behind `validate_document`: one function per area, run in a
//! fixed order so the first failure (and its message) is the same as a single linear pass.
use super::*;

/// Flow scope of a process or nested subprocess, flattened for the checks.
pub(super) struct Scope<'a> {
    pub(super) id: crate::ids::ElementSelector,
    pub(super) parent: Option<crate::ids::ElementSelector>,
    pub(super) nodes: &'a [Node],
    pub(super) lanes: &'a [Lane],
    pub(super) flows: &'a [Flow],
    pub(super) objects: &'a [crate::bpmn_data::DataObject],
    pub(super) associations: &'a [crate::bpmn_data::DataAssociation],
    pub(super) lane_set_id: Option<&'a crate::ids::ElementIdentity>,
    pub(super) groups: &'a [crate::bpmn_model::Group],
}

fn check_qname(name: &crate::bpmn_model::QNameRef) -> Result<(), String> {
    definitions_text(&name.namespace)?;
    if name.namespace.chars().any(char::is_whitespace) {
        return fail("Invalid QName namespace");
    }
    Ok(())
}

fn check_flow_message(flow: &Flow) -> Result<(), String> {
    if let Some(reference) = &flow.message_ref {
        if flow.kind != FlowKind::Message {
            return fail("messageRef requires a message flow");
        }
        if let crate::bpmn_activity::ModelReference::Qualified(qname) = reference {
            check_qname(qname)?;
        }
    }
    Ok(())
}

fn has_flow_node_refs(metadata: &Option<crate::bpmn_activity::FlowElementMetadata>) -> bool {
    metadata
        .as_ref()
        .is_some_and(|value| !value.incoming.is_empty() || !value.outgoing.is_empty())
}

/// Identity and reference state shared by the checks, filled in document order.
pub(super) struct DocumentChecks<'a> {
    d: &'a BpmnDocument,
    resources: &'a HashSet<&'a XmlId>,
    ids: HashSet<&'a crate::ids::ElementIdentity>,
    local_namespace: Option<&'a str>,
    reference_index: crate::reference_resolution::ReferenceIndex,
    root_by_selector: HashMap<crate::ids::ElementSelector, &'a crate::bpmn_model::EventDefinition>,
    root_by_xml: HashMap<&'a XmlId, &'a crate::bpmn_model::EventDefinition>,
    node_process: HashMap<crate::ids::ElementSelector, crate::ids::ElementSelector>,
    kinds: HashMap<crate::ids::ElementSelector, NodeKind>,
    flow_ids: HashSet<crate::ids::ElementSelector>,
    shape_ids: HashSet<crate::ids::ElementSelector>,
    group_ids: HashSet<crate::ids::ElementSelector>,
    element_scope: HashMap<crate::ids::ElementSelector, crate::ids::ElementSelector>,
    item_metadata: Vec<(&'a crate::bpmn_data::ItemAwareMetadata, Option<bool>)>,
    count: usize,
}

impl<'a> DocumentChecks<'a> {
    pub(super) fn new(
        d: &'a BpmnDocument,
        resources: &'a HashSet<&'a XmlId>,
    ) -> Result<Self, String> {
        let mut ids = HashSet::new();
        add(&mut ids, &d.definitions_id)?;
        let local_namespace = d
            .definitions
            .as_ref()
            .map(|v| v.target_namespace.as_str())
            .or_else(|| {
                d.provenance
                    .is_none()
                    .then_some(crate::model::GENERATED_TARGET_NAMESPACE)
            });
        let root_by_selector = d
            .root_event_definitions
            .iter()
            .filter_map(|definition| definition.id().map(|id| (id.selector(), definition)))
            .collect();
        let root_by_xml = d
            .root_event_definitions
            .iter()
            .filter_map(|definition| {
                definition
                    .id()
                    .and_then(|id| id.xml_id())
                    .map(|id| (id, definition))
            })
            .collect();
        Ok(Self {
            d,
            resources,
            ids,
            local_namespace,
            reference_index: crate::reference_resolution::ReferenceIndex::new(d),
            root_by_selector,
            root_by_xml,
            node_process: HashMap::new(),
            kinds: HashMap::new(),
            flow_ids: HashSet::new(),
            shape_ids: HashSet::new(),
            group_ids: HashSet::new(),
            element_scope: HashMap::new(),
            item_metadata: Vec::new(),
            count: 0,
        })
    }

    /// Distinct identities registered so far.
    pub(super) fn id_count(&self) -> usize {
        self.ids.len()
    }

    pub(super) fn check_scopes(&mut self, scopes: &[Scope<'a>]) -> Result<(), String> {
        for p in scopes {
            self.check_scope_lane_set(p)?;
            self.check_scope_groups(p)?;
            self.check_scope_data_objects(p)?;
            self.check_scope_data_associations(p)?;
            for n in p.nodes {
                self.check_node_flow_metadata(n)?;
                self.check_node_identity(p, n)?;
                self.check_node_activity(p, n)?;
                self.check_node_loop(n)?;
                self.check_node_details(p, n)?;
                self.register_node(p, n)?;
            }
            self.check_scope_lanes(p)?;
            self.check_scope_flows(p)?;
        }
        Ok(())
    }

    pub(super) fn check_choreographies(&mut self) -> Result<(), String> {
        let d = self.d;
        let target_namespace = d.definitions.as_ref().map(|v| v.target_namespace.as_str());
        for choreography in &d.choreographies {
            self.check_choreography_identity(choreography)?;
            self.check_choreography_conversation_tree(choreography)?;
            self.check_choreography_conversation_nodes(choreography)?;
            self.check_choreography_conversation_links(choreography)?;
            self.check_choreography_participants(choreography)?;
            self.check_choreography_message_flows(choreography)?;
            self.check_choreography_graph_mode(choreography)?;
            self.check_choreography_activities(choreography, target_namespace)?;
            self.check_choreography_sequence_flows(choreography)?;
        }
        Ok(())
    }

    pub(super) fn check_scope_endpoints(
        &mut self,
        scopes: &[Scope<'a>],
        scopes_by_id: &HashMap<crate::ids::ElementSelector, &Scope<'a>>,
    ) -> Result<(), String> {
        for p in scopes {
            self.check_scope_lane_hierarchy(p)?;
            self.check_scope_association_endpoints(p, scopes_by_id)?;
            self.check_scope_flow_endpoints(p)?;
        }
        Ok(())
    }

    pub(super) fn check_diagrams(&mut self, scopes: &[Scope<'a>]) -> Result<(), String> {
        let d = self.d;
        for diagram in &d.diagrams {
            self.check_diagram_header(diagram)?;
            let (choreography_scopes, contained) = self.check_diagram_plane(diagram, scopes)?;
            self.check_diagram_shapes(diagram, &choreography_scopes, &contained)?;
            self.check_diagram_edges(diagram, &choreography_scopes, &contained)?;
        }
        Ok(())
    }

    pub(super) fn check_definitions_declarations(&mut self) -> Result<(), String> {
        let d = self.d;
        let ids = &mut self.ids;
        if d.extensions.len() > MAX_COLLECTION_ITEMS || d.relationships.len() > MAX_COLLECTION_ITEMS
        {
            return fail("Definitions declaration limit exceeded");
        }
        for extension in &d.extensions {
            if extension.documentation.len() > MAX_COLLECTION_ITEMS {
                return fail("Extension documentation limit exceeded");
            }
            for doc in &extension.documentation {
                crate::bpmn_metadata::validate_documentation(doc)?;
            }
            if let Some(reference) = &extension.definition {
                check_qname(reference)?;
            }
        }
        for relationship in &d.relationships {
            if let Some(id) = &relationship.id {
                add(ids, id)?;
            }
            definitions_text(&relationship.relationship_type)?;
            if relationship.sources.len() > MAX_COLLECTION_ITEMS
                || relationship.targets.len() > MAX_COLLECTION_ITEMS
            {
                return fail("Relationship reference limit exceeded");
            }
            for reference in relationship.sources.iter().chain(&relationship.targets) {
                check_qname(reference)?;
            }
        }
        Ok(())
    }

    pub(super) fn check_item_definitions(&mut self) -> Result<(), String> {
        let d = self.d;
        let ids = &mut self.ids;
        let catalog = &d.catalog;
        if catalog.item_definitions.len() > MAX_COLLECTION_ITEMS
            || catalog.interfaces.len() > MAX_COLLECTION_ITEMS
            || catalog.resources.len() > MAX_COLLECTION_ITEMS
        {
            return fail("Supporting catalog limit exceeded");
        }
        for item in &catalog.item_definitions {
            if let Some(id) = &item.id {
                add(ids, id)?;
            }
            if let Some(name) = &item.structure_ref {
                check_qname(name)?;
            }
        }
        Ok(())
    }

    pub(super) fn check_interfaces(&mut self) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let catalog = &d.catalog;
        let local =
            |name: &crate::bpmn_model::QNameRef| local_namespace == Some(name.namespace.as_str());
        for interface in &catalog.interfaces {
            if let Some(id) = &interface.id {
                add(ids, id)?;
            }
            definitions_text(&interface.name)?;
            if let Some(name) = &interface.implementation_ref {
                check_qname(name)?;
            }
            if interface.operations.is_empty() || interface.operations.len() > MAX_COLLECTION_ITEMS
            {
                return fail("Interface requires one to 256 operations");
            }
            for operation in &interface.operations {
                if let Some(id) = &operation.id {
                    add(ids, id)?;
                }
                definitions_text(&operation.name)?;
                if let Some(name) = &operation.implementation_ref {
                    check_qname(name)?;
                }
                if operation.error_refs.len() > MAX_COLLECTION_ITEMS {
                    return fail("Operation error reference limit exceeded");
                }
                for reference in std::iter::once(&operation.in_message_ref)
                    .chain(operation.out_message_ref.iter())
                {
                    check_qname(reference)?;
                    if local(reference) && !d.event_resources.iter().any(|e| matches!(e, crate::bpmn_model::EventResource::Message { id, .. } if id.xml_id() == Some(&reference.local))) {
                        return fail("Operation references a missing local message");
                    }
                }
                for reference in &operation.error_refs {
                    check_qname(reference)?;
                    if local(reference) && !d.event_resources.iter().any(|e| matches!(e, crate::bpmn_model::EventResource::Error { id, .. } if id.xml_id() == Some(&reference.local))) {
                        return fail("Operation references a missing local error");
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_resources(&mut self) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let catalog = &d.catalog;
        let local =
            |name: &crate::bpmn_model::QNameRef| local_namespace == Some(name.namespace.as_str());
        for resource in &catalog.resources {
            if let Some(id) = &resource.id {
                add(ids, id)?;
            }
            definitions_text(&resource.name)?;
            if resource.parameters.len() > MAX_COLLECTION_ITEMS {
                return fail("Resource parameter limit exceeded");
            }
            for parameter in &resource.parameters {
                if let Some(id) = &parameter.id {
                    add(ids, id)?;
                }
                if let Some(name) = &parameter.name {
                    definitions_text(name)?;
                }
                if let Some(reference) = &parameter.type_ref {
                    check_qname(reference)?;
                    if local(reference)
                        && !catalog.item_definitions.iter().any(|item| {
                            item.id.as_ref().and_then(|id| id.xml_id()) == Some(&reference.local)
                        })
                    {
                        return fail(
                            "Resource parameter references a missing local item definition",
                        );
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn collect_item_metadata(&mut self) -> Result<(), String> {
        let d = self.d;
        let item_metadata = &mut self.item_metadata;
        for store in &d.data_stores {
            if let Some(meta) = &store.item_aware {
                item_metadata.push((meta, None));
            }
        }
        for process in &d.processes {
            if let Some(io) = &process.io_specification {
                for port in io.inputs.iter().chain(&io.outputs) {
                    if let Some(meta) = &port.item_aware {
                        item_metadata.push((meta, Some(port.is_collection.unwrap_or(false))));
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_flow_scopes(&mut self) -> Result<(), String> {
        let d = self.d;
        let ids = &mut self.ids;
        let item_metadata = &mut self.item_metadata;
        for scope in crate::queries::scopes(d) {
            for object in scope.data_objects {
                if has_flow_node_refs(&object.flow_metadata) {
                    return fail("incoming/outgoing references require a flow node");
                }
                if object
                    .flow_metadata
                    .as_ref()
                    .is_some_and(|m| m.is_immediate.is_some())
                {
                    return fail("isImmediate requires a sequence flow");
                }
                if let Some(meta) = &object.item_aware {
                    item_metadata.push((meta, Some(object.is_collection.unwrap_or(false))));
                }
            }
            for association in scope.data_associations {
                if association.assignments.len() > MAX_COLLECTION_ITEMS {
                    return fail("Data assignment limit exceeded");
                }
                for assignment in &association.assignments {
                    if let Some(id) = &assignment.id {
                        add(ids, id)?;
                    }
                }
            }
            for node in scope.nodes {
                if let Some(crate::bpmn_model::LoopCharacteristics::MultiInstance {
                    complex_behavior_definitions,
                    ..
                }) = &node.loop_characteristics
                {
                    for behavior in complex_behavior_definitions {
                        if behavior
                            .event
                            .as_ref()
                            .is_some_and(|event| has_flow_node_refs(&event.flow))
                        {
                            return fail(
                                "Implicit event flow references must use the event fields",
                            );
                        }
                        if behavior
                            .event
                            .as_ref()
                            .and_then(|event| event.flow.as_ref())
                            .is_some_and(|flow| flow.is_immediate.is_some())
                        {
                            return fail("isImmediate requires a sequence flow");
                        }
                    }
                }
                if let Some(crate::bpmn_model::NodeDetails::Event { properties, io, .. }) =
                    &node.details
                {
                    for property in properties {
                        if let Some(id) = &property.id {
                            add(ids, id)?;
                        }
                        if let Some(name) = &property.name {
                            definitions_text(name)?;
                        }
                        if let Some(metadata) = &property.item_aware {
                            item_metadata.push((metadata, None));
                        }
                    }
                    if let Some(io) = io {
                        use crate::bpmn_model::EventIo;
                        let (ports, set) = match io {
                            EventIo::Catch {
                                outputs,
                                output_set,
                            } => (outputs, output_set),
                            EventIo::Throw { inputs, input_set } => (inputs, input_set),
                        };
                        if ports.len() > MAX_COLLECTION_ITEMS {
                            return fail("Event port limit exceeded");
                        }
                        for port in ports {
                            if let Some(id) = &port.id {
                                add(ids, id)?;
                            }
                            if let Some(name) = &port.name {
                                definitions_text(name)?;
                            }
                            if let Some(metadata) = &port.item_aware {
                                item_metadata
                                    .push((metadata, Some(port.is_collection.unwrap_or(false))));
                            }
                        }
                        if let Some(set) = set {
                            if let Some(id) = &set.id {
                                add(ids, id)?;
                            }
                            if let Some(name) = &set.name {
                                definitions_text(name)?;
                            }
                            let mut seen = HashSet::new();
                            for reference in &set.data_refs {
                                if !seen.insert(reference)
                                    || !ports.iter().any(|port| {
                                        port.id.as_ref().is_some_and(|id| id == reference)
                                    })
                                {
                                    return fail("Invalid event set port reference");
                                }
                            }
                            if set
                                .optional_refs
                                .iter()
                                .chain(&set.while_executing_refs)
                                .any(|reference| !set.data_refs.contains(reference))
                                || !set.counterpart_refs.is_empty()
                            {
                                return fail("Invalid event set metadata");
                            }
                        }
                    }
                }
                if let Some(meta) = &node.item_aware {
                    if !matches!(
                        node.kind,
                        NodeKind::DataObjectReference | NodeKind::DataStoreReference
                    ) {
                        return fail("Item-aware metadata requires a data reference");
                    }
                    item_metadata.push((meta, None));
                }
                if let Some(io) = &node.io_specification {
                    for port in io.inputs.iter().chain(&io.outputs) {
                        if let Some(meta) = &port.item_aware {
                            item_metadata.push((meta, Some(port.is_collection.unwrap_or(false))));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_choreography_scope_events(&mut self) -> Result<(), String> {
        let d = self.d;
        let ids = &mut self.ids;
        let item_metadata = &mut self.item_metadata;
        // Event-owned properties, ports, sets, and data states in choreography
        // graphs participate in the same global identity and QName checks as process events.
        for scope in crate::queries::choreography_scopes(d) {
            if scope.root.graph.is_none() {
                continue;
            }
            for node in scope.events {
                if let Some(crate::bpmn_model::NodeDetails::Event { properties, io, .. }) =
                    &node.details
                {
                    if properties.len() > MAX_COLLECTION_ITEMS {
                        return fail("Event property limit exceeded");
                    }
                    for property in properties {
                        if let Some(id) = &property.id {
                            add(ids, id)?;
                        }
                        if let Some(name) = &property.name {
                            definitions_text(name)?;
                        }
                        if let Some(metadata) = &property.item_aware {
                            item_metadata.push((metadata, None));
                        }
                    }
                    if let Some(io) = io {
                        use crate::bpmn_model::EventIo;
                        let (ports, set) = match io {
                            EventIo::Catch {
                                outputs,
                                output_set,
                            } => (outputs, output_set),
                            EventIo::Throw { inputs, input_set } => (inputs, input_set),
                        };
                        if ports.len() > MAX_COLLECTION_ITEMS {
                            return fail("Event port limit exceeded");
                        }
                        for port in ports {
                            if let Some(id) = &port.id {
                                add(ids, id)?;
                            }
                            if let Some(name) = &port.name {
                                definitions_text(name)?;
                            }
                            if let Some(metadata) = &port.item_aware {
                                item_metadata
                                    .push((metadata, Some(port.is_collection.unwrap_or(false))));
                            }
                        }
                        if let Some(set) = set {
                            if let Some(id) = &set.id {
                                add(ids, id)?;
                            }
                            if let Some(name) = &set.name {
                                definitions_text(name)?;
                            }
                            let mut seen = HashSet::new();
                            for reference in &set.data_refs {
                                if !seen.insert(reference)
                                    || !ports.iter().any(|port| {
                                        port.id.as_ref().is_some_and(|id| id == reference)
                                    })
                                {
                                    return fail("Invalid event set port reference");
                                }
                            }
                            if set
                                .optional_refs
                                .iter()
                                .chain(&set.while_executing_refs)
                                .any(|r| !set.data_refs.contains(r))
                                || !set.counterpart_refs.is_empty()
                            {
                                return fail("Invalid event set metadata");
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_item_aware_metadata(&mut self) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let item_metadata = std::mem::take(&mut self.item_metadata);
        let catalog = &d.catalog;
        let local =
            |name: &crate::bpmn_model::QNameRef| local_namespace == Some(name.namespace.as_str());
        for (metadata, collection) in item_metadata {
            if let Some(state) = &metadata.data_state {
                if let Some(id) = &state.id {
                    add(ids, id)?;
                }
                if let Some(name) = &state.name {
                    definitions_text(name)?;
                }
            }
            if let Some(reference) = &metadata.item_subject_ref {
                check_qname(reference)?;
                if local(reference) {
                    let Some(item) = catalog.item_definitions.iter().find(|item| {
                        item.id.as_ref().and_then(|id| id.xml_id()) == Some(&reference.local)
                    }) else {
                        return fail("Missing local item definition");
                    };
                    if let Some(value) = collection
                        && value != item.is_collection.unwrap_or(false)
                    {
                        return fail("Collection status differs from item definition");
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_expressions(
        &mut self,
        formal_expressions: Vec<&crate::bpmn_model::Expression>,
    ) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let catalog = &d.catalog;
        let local =
            |name: &crate::bpmn_model::QNameRef| local_namespace == Some(name.namespace.as_str());
        let transformations: Vec<_> = crate::queries::scopes(d)
            .into_iter()
            .flat_map(|scope| {
                scope
                    .data_associations
                    .iter()
                    .filter_map(|association| association.transformation.as_ref())
            })
            .chain(
                crate::queries::choreography_scopes(d)
                    .into_iter()
                    .flat_map(|scope| {
                        scope
                            .data_associations
                            .into_iter()
                            .filter_map(|association| association.transformation.as_ref())
                    }),
            )
            .collect();
        for expression in d.expressions() {
            use crate::bpmn_model::{Expression, ExpressionKind};
            let transformation = transformations
                .iter()
                .chain(&formal_expressions)
                .any(|candidate| std::ptr::eq(*candidate, expression));
            if transformation
                && !matches!(
                    expression,
                    Expression::Structured(crate::bpmn_model::ExpressionData {
                        kind: ExpressionKind::Formal { .. },
                        ..
                    })
                )
            {
                return fail("Data transformation requires a formal expression");
            }
            if let Expression::Structured(data) = expression {
                if let Some(id) = &data.id {
                    add(ids, id)?;
                }
                if let Some(text) = match &data.text {
                    crate::bpmn_model::ExpressionBody::Text(s) => Some(s.as_str()),
                    _ => None,
                } {
                    definitions_text(text)?;
                }
                if data.namespaces.len() > MAX_COLLECTION_ITEMS {
                    return fail("Expression namespace limit exceeded");
                }
                let mut prefixes = HashSet::new();
                for (prefix, uri) in &data.namespaces {
                    definitions_text(prefix)?;
                    definitions_text(uri)?;
                    if !prefixes.insert(prefix)
                        || uri.chars().any(char::is_whitespace)
                        || prefix == "xmlns"
                        || (!prefix.is_empty()
                            && (prefix.parse::<XmlId>().is_err() || uri.is_empty()))
                        || (prefix == "xml") != (uri == "http://www.w3.org/XML/1998/namespace")
                        || uri == "http://www.w3.org/2000/xmlns/"
                    {
                        return fail("Invalid expression namespace binding");
                    }
                }
                if !data
                    .namespaces
                    .iter()
                    .any(|(_, uri)| uri == "http://www.omg.org/spec/BPMN/20100524/MODEL")
                {
                    return fail("Expression namespace context cannot represent a BPMN element");
                }
                let explicit_type = match &data.kind {
                    ExpressionKind::Base { explicit_type }
                    | ExpressionKind::Formal { explicit_type, .. } => *explicit_type,
                };
                if explicit_type
                    && !data.namespaces.iter().any(|(prefix, uri)| {
                        !prefix.is_empty() && uri == "http://www.w3.org/2001/XMLSchema-instance"
                    })
                {
                    return fail("Expression namespace context cannot represent xsi:type");
                }
                if let ExpressionKind::Formal {
                    explicit_type,
                    language,
                    evaluates_to_type_ref,
                    ..
                } = &data.kind
                {
                    if !explicit_type && !transformation {
                        return fail(
                            "Formal expression requires an explicit xsi:type in this editable profile",
                        );
                    }
                    if let Some(language) = language {
                        definitions_text(language)?;
                    }
                    if let Some(name) = evaluates_to_type_ref {
                        definitions_text(&name.namespace)?;
                        if local(name)
                            && !catalog.item_definitions.iter().any(|item| {
                                item.id.as_ref().and_then(|id| id.xml_id()) == Some(&name.local)
                            })
                        {
                            return fail("Expression references a missing local item definition");
                        }
                        let default = data
                            .namespaces
                            .iter()
                            .find(|(prefix, _)| prefix.is_empty())
                            .map(|(_, uri)| uri.as_str())
                            .unwrap_or("");
                        if (name.namespace.is_empty() && !default.is_empty())
                            || (!name.namespace.is_empty()
                                && !data
                                    .namespaces
                                    .iter()
                                    .any(|(_, uri)| uri == &name.namespace))
                        {
                            return fail(
                                "Expression namespace context cannot represent evaluatesToTypeRef",
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_event_resources(&mut self) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let catalog = &d.catalog;
        let local =
            |name: &crate::bpmn_model::QNameRef| local_namespace == Some(name.namespace.as_str());
        use crate::bpmn_model::EventResource;
        for resource in &d.event_resources {
            let (id, name) = match resource {
                EventResource::Signal { id, name, .. }
                | EventResource::Message { id, name, .. }
                | EventResource::Error { id, name, .. }
                | EventResource::Escalation { id, name, .. } => (id, name),
            };
            add(ids, id)?;
            if let Some(name) = name {
                short(name)?;
            }
            let item = match resource {
                EventResource::Message { item_ref, .. } => item_ref,
                EventResource::Signal { structure_ref, .. }
                | EventResource::Error { structure_ref, .. }
                | EventResource::Escalation { structure_ref, .. } => structure_ref,
            };
            if let Some(reference) = item {
                check_qname(reference)?;
                if local(reference)
                    && !catalog.item_definitions.iter().any(|item| {
                        item.id.as_ref().and_then(|id| id.xml_id()) == Some(&reference.local)
                    })
                {
                    return fail("Event resource references a missing local item definition");
                }
            }
            match resource {
                EventResource::Error {
                    code: Some(code), ..
                }
                | EventResource::Escalation {
                    code: Some(code), ..
                } => short(code)?,
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn check_root_event_definitions(&mut self) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        if d.root_event_definitions.len() > MAX_COLLECTION_ITEMS {
            return fail("Root event definition limit exceeded");
        }
        for definition in &d.root_event_definitions {
            if let Some(id) = definition.id() {
                add(ids, id)?;
            }
            validate_event_payload(d, definition, local_namespace)?;
        }
        Ok(())
    }

    pub(super) fn check_provenance(&self) -> Result<(), String> {
        let d = self.d;
        let resources = self.resources;
        if let Some(p) = &d.provenance
            && !resources.contains(&p.source_resource_id)
        {
            return fail("missing provenance resource");
        }
        Ok(())
    }

    pub(super) fn check_capability(&self) -> Result<(), String> {
        let d = self.d;
        let resources = self.resources;
        match &d.capability {
            Capability::Editable => {}
            Capability::PreservedSource { source_resource_id }
            | Capability::RetainedExtensions {
                source_resource_id, ..
            } => {
                if !resources.contains(source_resource_id) {
                    return fail("missing source resource");
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_categories(&mut self) -> Result<(), String> {
        let d = self.d;
        let ids = &mut self.ids;
        for category in &d.categories {
            add(ids, &category.id)?;
            if let Some(name) = &category.name {
                short(name)?;
            }
            for value in &category.values {
                add(ids, &value.id)?;
                if let Some(text) = &value.value {
                    short(text)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_data_stores(&mut self) -> Result<(), String> {
        let d = self.d;
        let ids = &mut self.ids;
        for store in &d.data_stores {
            if let Some(id) = &store.id {
                add(ids, id)?;
            }
            if let Some(name) = &store.name {
                short(name)?;
            }
            if let Some(capacity) = &store.capacity {
                let digits = capacity.strip_prefix(['+', '-']).unwrap_or(capacity);
                if capacity.len() > MAX_TEXT
                    || digits.is_empty()
                    || !digits.bytes().all(|b| b.is_ascii_digit())
                {
                    return fail("invalid data store capacity");
                }
            }
        }
        Ok(())
    }

    pub(super) fn collect_scopes(&mut self) -> Result<Vec<Scope<'a>>, String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let mut count = self.count;
        let reference_index = &self.reference_index;
        let local =
            |name: &crate::bpmn_model::QNameRef| local_namespace == Some(name.namespace.as_str());
        let endpoint =|reference: &Option<crate::bpmn_activity::ModelReference>| -> Option<crate::ids::ElementSelector> {
            reference.as_ref().and_then(|reference| match reference_index.resolve(reference) { crate::reference_resolution::ReferenceResolution::Resolved(value) => Some(value), _ => None })
        };
        let mut scopes: Vec<Scope<'_>> = Vec::new();
        let mut pending: Vec<(Scope<'_>, usize)> = d
            .processes
            .iter()
            .map(|p| {
                (
                    Scope {
                        id: p.id.selector(),
                        parent: None,
                        nodes: &p.nodes,
                        lanes: &p.lanes,
                        flows: &p.flows,
                        objects: &p.data_objects,
                        associations: &p.data_associations,
                        lane_set_id: p.lane_set_id.as_ref(),
                        groups: &p.groups,
                    },
                    0,
                )
            })
            .collect();
        while let Some((scope, depth)) = pending.pop() {
            if depth > 32 {
                return fail("subprocess depth limit exceeded");
            }
            count = count
                .saturating_add(scope.nodes.len())
                .saturating_add(scope.flows.len())
                .saturating_add(scope.lanes.len())
                .saturating_add(scope.objects.len())
                .saturating_add(scope.associations.len())
                .saturating_add(scope.groups.len());
            if count > MAX_DOCUMENT_ELEMENTS {
                return fail("document element limit exceeded");
            }
            for n in scope.nodes {
                if let Some(crate::bpmn_model::NodeDetails::DataStoreReference {
                    data_store_ref: Some(crate::bpmn_data::DataStoreRef::Qualified(reference)),
                }) = &n.details
                {
                    check_qname(reference)?;
                }
                match (&n.kind, &n.details) {
                    (
                        NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction,
                        Some(crate::bpmn_model::NodeDetails::SubProcess { content }),
                    ) => pending.push((
                        Scope {
                            id: n.id.selector(),
                            parent: Some(scope.id.clone()),
                            nodes: &content.nodes,
                            lanes: &content.lanes,
                            flows: &content.flows,
                            objects: &content.data_objects,
                            associations: &content.data_associations,
                            lane_set_id: content.lane_set_id.as_ref(),
                            groups: &content.groups,
                        },
                        depth + 1,
                    )),
                    (
                        NodeKind::CallActivity,
                        Some(crate::bpmn_model::NodeDetails::CallActivity { called_element }),
                    ) => {
                        if called_element
                            .as_ref()
                            .is_some_and(|value| value.namespace.len() > MAX_TEXT)
                        {
                            return fail("invalid call namespace");
                        }
                    }
                    (
                        NodeKind::SubProcess
                        | NodeKind::AdHocSubProcess
                        | NodeKind::Transaction
                        | NodeKind::CallActivity,
                        _,
                    ) => return fail("activity details do not match kind"),
                    (
                        NodeKind::DataObjectReference,
                        Some(crate::bpmn_model::NodeDetails::DataObjectReference {
                            data_object_ref,
                        }),
                    ) if data_object_ref.as_ref().is_none_or(|id| {
                        if scope
                            .objects
                            .iter()
                            .any(|o| o.id.as_ref().is_some_and(|identity| identity == id))
                        {
                            return true;
                        }
                        let mut parent = scope.parent.clone();
                        while let Some(scope_id) = parent {
                            let Some(ancestor) =
                                scopes.iter().find(|candidate| candidate.id == scope_id)
                            else {
                                break;
                            };
                            if ancestor
                                .objects
                                .iter()
                                .any(|o| o.id.as_ref().is_some_and(|identity| identity == id))
                            {
                                return true;
                            }
                            parent = ancestor.parent.clone();
                        }
                        false
                    }) => {}
                    (
                        NodeKind::DataStoreReference,
                        Some(crate::bpmn_model::NodeDetails::DataStoreReference { data_store_ref }),
                    ) if data_store_ref
                        .as_ref()
                        .is_none_or(|reference| match reference {
                            crate::bpmn_data::DataStoreRef::Legacy(id) => d
                                .data_stores
                                .iter()
                                .any(|o| o.id.as_ref().is_some_and(|identity| identity == id)),
                            crate::bpmn_data::DataStoreRef::Qualified(name) => {
                                !local(name)
                                    || d.data_stores.iter().any(|o| {
                                        o.id.as_ref().and_then(|identity| identity.xml_id())
                                            == Some(&name.local)
                                    })
                            }
                        }) => {}
                    (
                        NodeKind::ExclusiveGateway
                        | NodeKind::InclusiveGateway
                        | NodeKind::ParallelGateway
                        | NodeKind::ComplexGateway
                        | NodeKind::EventBasedGateway,
                        Some(crate::bpmn_model::NodeDetails::Gateway {
                            default_flow,
                            instantiate,
                            event_gateway_type,
                            activation_condition,
                            ..
                        }),
                    ) => {
                        if default_flow.is_some()
                            && !matches!(
                                n.kind,
                                NodeKind::ExclusiveGateway
                                    | NodeKind::InclusiveGateway
                                    | NodeKind::ComplexGateway
                            )
                        {
                            return fail("gateway kind cannot have default flow");
                        }
                        if default_flow.as_ref().is_some_and(|id| {
                            !scope.flows.iter().any(|f| {
                                &f.id == id
                                    && endpoint(&f.source).is_some_and(|source| source == n.id)
                                    && f.kind == FlowKind::Sequence
                            })
                        }) {
                            return fail(
                                "gateway default must be an outgoing flow in the same scope",
                            );
                        }
                        if (instantiate.is_some() || event_gateway_type.is_some())
                            && !matches!(n.kind, NodeKind::EventBasedGateway)
                        {
                            return fail("event gateway attributes on wrong gateway kind");
                        }
                        if let Some(condition) = activation_condition
                            && (!matches!(n.kind, NodeKind::ComplexGateway)
                                || condition.plain_text().is_some_and(|s| s.len() > MAX_TEXT))
                        {
                            return fail("invalid gateway activation condition");
                        }
                    }
                    (_, Some(crate::bpmn_model::NodeDetails::Gateway { .. })) => {
                        return fail("gateway details require gateway");
                    }
                    (
                        NodeKind::TextAnnotation,
                        Some(crate::bpmn_model::NodeDetails::TextAnnotation { text_format, text }),
                    ) => {
                        if let Some(format) = text_format {
                            short(format)?;
                        }
                        if let Some(text) = text {
                            crate::bpmn_xml::validate_mixed(text)?;
                        }
                    }
                    (_, Some(crate::bpmn_model::NodeDetails::TextAnnotation { .. })) => {
                        return fail("annotation details require text annotation");
                    }
                    (NodeKind::DataStoreReference, _) => {
                        return fail("data store reference requires matching details");
                    }
                    (
                        _,
                        Some(
                            crate::bpmn_model::NodeDetails::SubProcess { .. }
                            | crate::bpmn_model::NodeDetails::CallActivity { .. }
                            | crate::bpmn_model::NodeDetails::DataObjectReference { .. }
                            | crate::bpmn_model::NodeDetails::DataStoreReference { .. },
                        ),
                    ) => return fail("activity details do not match kind"),
                    _ => (),
                }
            }
            scopes.push(scope);
        }
        self.count = count;
        Ok(scopes)
    }

    pub(super) fn register_data_store_shapes(&mut self) -> Result<(), String> {
        let d = self.d;
        let shape_ids = &mut self.shape_ids;
        for store in &d.data_stores {
            if let Some(id) = &store.id {
                shape_ids.insert(id.selector());
            }
        }
        Ok(())
    }

    pub(super) fn check_collaboration_extras(&mut self) -> Result<(), String> {
        let d = self.d;
        let ids = &mut self.ids;
        let flow_ids = &mut self.flow_ids;
        let shape_ids = &mut self.shape_ids;
        let group_ids = &mut self.group_ids;
        let element_scope = &mut self.element_scope;
        let collaboration_extras = d
            .collaborations()
            .map(|c| (&c.id, &c.extras))
            .chain(d.choreographies.iter().map(|c| (&c.id, &c.extras)));
        for (owner, extras) in collaboration_extras {
            if let Some(extras) = extras {
                for artifact in &extras.artifacts {
                    use crate::bpmn_collaboration::CollaborationArtifact;
                    match artifact {
                        CollaborationArtifact::Group(group) => {
                            add(ids, &group.id)?;
                            group_ids.insert(group.id.selector());
                            shape_ids.insert(group.id.selector());
                            element_scope.insert(group.id.selector(), owner.selector());
                        }
                        CollaborationArtifact::TextAnnotation(node) => {
                            if node.kind != NodeKind::TextAnnotation
                                || node.activity.is_some()
                                || node.io_specification.is_some()
                                || node.loop_characteristics.is_some()
                                || node.item_aware.is_some()
                                || node.flow_metadata.is_some()
                                || node.details.as_ref().is_some_and(|details| {
                                    !matches!(
                                        details,
                                        crate::bpmn_model::NodeDetails::TextAnnotation { .. }
                                    )
                                })
                            {
                                return fail(
                                    "Collaboration text annotation has incompatible metadata",
                                );
                            }
                            add(ids, &node.id)?;
                            shape_ids.insert(node.id.selector());
                            element_scope.insert(node.id.selector(), owner.selector());
                        }
                        CollaborationArtifact::Association(flow) => {
                            if flow.kind != FlowKind::Association
                                || flow.condition.is_some()
                                || flow.message_ref.is_some()
                                || flow.flow_metadata.is_some()
                                || flow.name.is_some()
                            {
                                return fail("Collaboration association has incompatible metadata");
                            }
                            add(ids, &flow.id)?;
                            flow_ids.insert(flow.id.selector());
                            element_scope.insert(flow.id.selector(), owner.selector());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_processes(&mut self) -> Result<(), String> {
        let d = self.d;
        let ids = &mut self.ids;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        for process in &d.processes {
            add(ids, &process.id)?;
            if let Some(name) = &process.name {
                short(name)?;
            }
            if let Some(io) = &process.io_specification {
                validate_io(io, ids, shape_ids, element_scope, &process.id.selector())?;
            }
        }
        Ok(())
    }

    pub(super) fn check_scope_lane_set(&mut self, p: &Scope<'a>) -> Result<(), String> {
        let ids = &mut self.ids;
        if let Some(id) = p.lane_set_id {
            add(ids, id)?;
        }
        Ok(())
    }

    pub(super) fn check_scope_groups(&mut self, p: &Scope<'a>) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let node_process = &mut self.node_process;
        let shape_ids = &mut self.shape_ids;
        let group_ids = &mut self.group_ids;
        let element_scope = &mut self.element_scope;
        for group in p.groups {
            add(ids, &group.id)?;
            shape_ids.insert(group.id.selector());
            group_ids.insert(group.id.selector());
            node_process.insert(group.id.selector(), p.id.clone());
            element_scope.insert(group.id.selector(), p.id.clone());
            if group.category_value_ref.as_ref().is_some_and(|id| {
                !id.is_external(local_namespace)
                    && !d.categories.iter().any(|c| {
                        c.values
                            .iter()
                            .any(|v| id.matches_identity(&v.id, local_namespace))
                    })
            }) {
                return fail("missing group category value");
            }
        }
        Ok(())
    }

    pub(super) fn check_scope_data_objects(&mut self, p: &Scope<'a>) -> Result<(), String> {
        let ids = &mut self.ids;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        for object in p.objects {
            if let Some(id) = &object.id {
                add(ids, id)?;
                shape_ids.insert(id.selector());
                element_scope.insert(id.selector(), p.id.clone());
            }
            if let Some(name) = &object.name {
                short(name)?;
            }
        }
        Ok(())
    }

    pub(super) fn check_scope_data_associations(&mut self, p: &Scope<'a>) -> Result<(), String> {
        let ids = &mut self.ids;
        let flow_ids = &mut self.flow_ids;
        let element_scope = &mut self.element_scope;
        for association in p.associations {
            if let Some(id) = &association.id {
                add(ids, id)?;
            }
            if let Some(id) = &association.id {
                flow_ids.insert(id.selector());
                element_scope.insert(id.selector(), p.id.clone());
            }
        }
        Ok(())
    }

    pub(super) fn check_node_flow_metadata(&self, n: &'a Node) -> Result<(), String> {
        if n.flow_metadata
            .as_ref()
            .is_some_and(|m| m.is_immediate.is_some())
        {
            return fail("isImmediate requires a sequence flow");
        }
        if matches!(
            n.kind,
            NodeKind::DataObjectReference | NodeKind::DataStoreReference
        ) && has_flow_node_refs(&n.flow_metadata)
        {
            return fail("incoming/outgoing references require a flow node");
        }
        if n.flow_metadata.is_some() && matches!(n.kind, NodeKind::TextAnnotation) {
            return fail("Flow-element metadata requires a flow element");
        }
        Ok(())
    }

    pub(super) fn check_node_identity(&mut self, p: &Scope<'a>, n: &'a Node) -> Result<(), String> {
        let ids = &mut self.ids;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        add(ids, &n.id)?;
        if let Some(io) = &n.io_specification {
            if !n.kind.is_activity() {
                return fail("I/O specification requires activity");
            }
            validate_io(io, ids, shape_ids, element_scope, &p.id)?;
        }
        if let Some(name) = &n.name {
            short(name)?;
        }
        Ok(())
    }

    pub(super) fn check_node_activity(&self, p: &Scope<'a>, n: &'a Node) -> Result<(), String> {
        let reference_index = &self.reference_index;
        let endpoint =|reference: &Option<crate::bpmn_activity::ModelReference>| -> Option<crate::ids::ElementSelector> {
            reference.as_ref().and_then(|reference| match reference_index.resolve(reference) { crate::reference_resolution::ReferenceResolution::Resolved(value) => Some(value), _ => None })
        };
        if let Some(activity) = &n.activity {
            if !n.kind.is_activity() {
                return fail("activity properties require activity");
            }
            let check_text = |value: &Option<String>| -> Result<(), String> {
                if value.as_ref().is_some_and(|v| v.len() > MAX_TEXT) {
                    return fail("activity property too long");
                }
                Ok(())
            };
            for value in [&activity.start_quantity, &activity.completion_quantity] {
                check_text(value)?;
                if let Some(value) = value {
                    let trimmed = value.trim();
                    let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
                    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                        return fail("invalid activity quantity");
                    }
                }
            }
            if activity.default_flow.as_ref().is_some_and(|id| {
                !p.flows.iter().any(|f| {
                    &f.id == id
                        && endpoint(&f.source).is_some_and(|source| source == n.id)
                        && f.kind == FlowKind::Sequence
                })
            }) {
                return fail("activity default must be outgoing flow in same scope");
            }
            if activity.triggered_by_event.is_some()
                && !matches!(
                    n.kind,
                    NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction
                )
            {
                return fail("event trigger requires subprocess");
            }
            if (activity.ad_hoc_ordering.is_some()
                || activity.cancel_remaining_instances.is_some()
                || activity.completion_condition.is_some())
                && !matches!(n.kind, NodeKind::AdHocSubProcess)
            {
                return fail("ad hoc metadata requires ad hoc subprocess");
            }
            if activity.transaction_method.is_some() && !matches!(n.kind, NodeKind::Transaction) {
                return fail("transaction method requires transaction");
            }
            if activity.implementation.is_some()
                && !matches!(
                    n.kind,
                    NodeKind::ServiceTask
                        | NodeKind::SendTask
                        | NodeKind::ReceiveTask
                        | NodeKind::UserTask
                        | NodeKind::BusinessRuleTask
                )
            {
                return fail("implementation on unsupported activity");
            }
            if activity.message_ref.is_some()
                && !matches!(n.kind, NodeKind::SendTask | NodeKind::ReceiveTask)
            {
                return fail("message reference requires send or receive task");
            }
            if activity.operation_ref.is_some()
                && !matches!(
                    n.kind,
                    NodeKind::ServiceTask | NodeKind::SendTask | NodeKind::ReceiveTask
                )
            {
                return fail("operation reference requires service, send or receive task");
            }
            if activity.instantiate.is_some() && !matches!(n.kind, NodeKind::ReceiveTask) {
                return fail("instantiate requires receive task");
            }
            if (activity.script_format.is_some() || activity.script.is_some())
                && !matches!(n.kind, NodeKind::ScriptTask)
            {
                return fail("script metadata requires script task");
            }
            for value in [
                &activity.transaction_method,
                &activity.implementation,
                &activity.script_format,
            ] {
                check_text(value)?;
            }
            if let Some(script) = &activity.script {
                match script {
                    crate::bpmn_xml::MixedXml::Text(text) => {
                        if text.len() > MAX_TEXT {
                            return fail("activity property too long");
                        }
                    }
                    crate::bpmn_xml::MixedXml::Structured(_) => {
                        crate::bpmn_xml::validate_mixed(script)?
                    }
                }
            }
            if let Some(value) = &activity.completion_condition
                && let Some(text) = value.plain_text()
            {
                short(text)?;
            }
            for name in [&activity.message_ref, &activity.operation_ref]
                .into_iter()
                .flatten()
            {
                if name.namespace.len() > MAX_TEXT {
                    return fail("invalid QName namespace");
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_node_loop(&self, n: &'a Node) -> Result<(), String> {
        if let Some(characteristics) = &n.loop_characteristics {
            use crate::bpmn_model::LoopCharacteristics;
            if !n.kind.is_activity() {
                return fail("loop characteristics require an activity");
            }
            let check = |value: Option<&str>| -> Result<(), String> {
                if value.is_some_and(|s| s.len() > MAX_TEXT) {
                    return fail("invalid loop expression");
                }
                Ok(())
            };
            match characteristics {
                LoopCharacteristics::Standard {
                    condition, maximum, ..
                } => {
                    check(condition.as_ref().and_then(|e| e.plain_text()))?;
                    check(maximum.as_deref())?;
                    if let Some(value) = maximum {
                        let trimmed = value.trim();
                        let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
                        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                            return fail("invalid loop maximum");
                        }
                    }
                }
                LoopCharacteristics::MultiInstance {
                    cardinality,
                    completion_condition,
                    ..
                } => {
                    check(cardinality.as_ref().and_then(|e| e.plain_text()))?;
                    check(completion_condition.as_ref().and_then(|e| e.plain_text()))?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_node_details(&mut self, p: &Scope<'a>, n: &'a Node) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let root_by_selector = &self.root_by_selector;
        let root_by_xml = &self.root_by_xml;
        if let Some(details) = &n.details
            && !matches!(
                details,
                crate::bpmn_model::NodeDetails::SubProcess { .. }
                    | crate::bpmn_model::NodeDetails::CallActivity { .. }
                    | crate::bpmn_model::NodeDetails::DataObjectReference { .. }
                    | crate::bpmn_model::NodeDetails::DataStoreReference { .. }
                    | crate::bpmn_model::NodeDetails::Gateway { .. }
                    | crate::bpmn_model::NodeDetails::TextAnnotation { .. }
            )
        {
            use crate::bpmn_model::{EventDefinition, NodeDetails};
            let (
                attached,
                definitions,
                cancel,
                definition_refs,
                parallel_multiple,
                is_interrupting,
                properties,
                event_io,
            ) = match details {
                NodeDetails::BoundaryEvent {
                    attached_to,
                    cancel_activity,
                } => (
                    Some(attached_to),
                    &[][..],
                    Some(*cancel_activity),
                    &[][..],
                    None,
                    None,
                    &[][..],
                    None,
                ),
                NodeDetails::SubProcess { .. }
                | NodeDetails::CallActivity { .. }
                | NodeDetails::DataObjectReference { .. }
                | NodeDetails::DataStoreReference { .. }
                | NodeDetails::Gateway { .. }
                | NodeDetails::TextAnnotation { .. } => {
                    unreachable!()
                }
                NodeDetails::Event {
                    definitions,
                    attached_to,
                    cancel_activity,
                    definition_refs,
                    parallel_multiple,
                    is_interrupting,
                    properties,
                    io,
                } => (
                    attached_to.as_ref(),
                    definitions.as_slice(),
                    *cancel_activity,
                    definition_refs.as_slice(),
                    *parallel_multiple,
                    *is_interrupting,
                    properties.as_slice(),
                    io.as_ref(),
                ),
            };
            let event = matches!(
                n.kind,
                NodeKind::StartEvent
                    | NodeKind::EndEvent
                    | NodeKind::IntermediateCatchEvent
                    | NodeKind::IntermediateThrowEvent
                    | NodeKind::BoundaryEvent
            );
            if !event
                || definitions.len() > MAX_COLLECTION_ITEMS
                || definition_refs.len() > MAX_COLLECTION_ITEMS
                || properties.len() > MAX_COLLECTION_ITEMS
            {
                return fail("invalid event details");
            }
            let catch = matches!(
                n.kind,
                NodeKind::StartEvent | NodeKind::IntermediateCatchEvent | NodeKind::BoundaryEvent
            );
            let event_sub_start = n.kind == NodeKind::StartEvent
                && crate::queries::find_node(d, &p.id).is_some_and(|owner| {
                    owner
                        .activity
                        .as_ref()
                        .is_some_and(|a| a.triggered_by_event == Some(true))
                });
            if parallel_multiple.is_some() && !catch {
                return fail("parallelMultiple requires a catch event");
            }
            if is_interrupting.is_some() && n.kind != NodeKind::StartEvent {
                return fail("isInterrupting requires a start event");
            }
            if event_io.is_some_and(|io| {
                !matches!(
                    (catch, io),
                    (true, crate::bpmn_model::EventIo::Catch { .. })
                        | (false, crate::bpmn_model::EventIo::Throw { .. })
                )
            }) {
                return fail("Event I/O direction does not match event kind");
            }
            if matches!(n.kind, NodeKind::BoundaryEvent) {
                let Some(target) = attached else {
                    return fail("boundary event requires attachment");
                };
                if !target.is_external(local_namespace)
                    && !p.nodes.iter().any(|other| {
                        target.matches_identity(&other.id, local_namespace)
                            && other.kind.is_activity()
                    })
                {
                    return fail("invalid boundary attachment");
                }
            } else if attached.is_some() || cancel.is_some() {
                return fail("attachment only allowed on boundary events");
            }
            let mut resolved = Vec::new();
            let mut seen_references = HashSet::new();
            let mut seen_local_roots = HashSet::new();
            for reference in definition_refs {
                if !seen_references.insert(reference) {
                    return fail("Duplicate event definition reference");
                }
                if let crate::bpmn_model::EventReference::Qualified(name) = reference {
                    check_qname(name)?;
                }
                if reference.is_external(local_namespace) {
                    continue;
                }
                let definition = match reference {
                    crate::bpmn_model::EventReference::Local(selector) => {
                        root_by_selector.get(selector).copied()
                    }
                    crate::bpmn_model::EventReference::Qualified(name) => {
                        root_by_xml.get(&name.local).copied()
                    }
                }
                .ok_or("Missing or wrong-kind root event definition")?;
                if let Some(id) = definition.id()
                    && !seen_local_roots.insert(id.selector())
                {
                    return fail("Duplicate event definition reference");
                }
                resolved.push(definition);
            }
            for (definition, owned) in definitions
                .iter()
                .map(|d| (d, true))
                .chain(resolved.into_iter().map(|d| (d, false)))
            {
                validate_event_payload(d, definition, local_namespace)?;
                match definition {
                    EventDefinition::Timer { id, .. } => {
                        if !catch {
                            return fail("timer definition requires a catch event");
                        }
                        if owned && let Some(id) = id {
                            add(ids, id)?;
                        }
                    }
                    EventDefinition::Signal { id, .. } | EventDefinition::Message { id, .. } => {
                        if owned && let Some(id) = id {
                            add(ids, id)?;
                        }
                    }
                    EventDefinition::Error { id, .. } => {
                        if !matches!(n.kind, NodeKind::EndEvent | NodeKind::BoundaryEvent)
                            && !event_sub_start
                        {
                            return fail("invalid error event context");
                        }
                        if owned && let Some(id) = id {
                            add(ids, id)?;
                        }
                    }
                    EventDefinition::Escalation { id, .. } => {
                        if !matches!(
                            n.kind,
                            NodeKind::EndEvent
                                | NodeKind::IntermediateThrowEvent
                                | NodeKind::BoundaryEvent
                        ) && !event_sub_start
                        {
                            return fail("invalid escalation event context");
                        }
                        if owned && let Some(id) = id {
                            add(ids, id)?;
                        }
                    }
                    EventDefinition::Conditional { id, expression, .. } => {
                        if !matches!(
                            n.kind,
                            NodeKind::StartEvent
                                | NodeKind::IntermediateCatchEvent
                                | NodeKind::BoundaryEvent
                        ) {
                            return fail("conditional definition requires a catch event");
                        }
                        if owned && let Some(id) = id {
                            add(ids, id)?;
                        }
                        if expression.plain_text().is_some_and(|s| s.len() > MAX_TEXT) {
                            return fail("invalid conditional expression");
                        }
                    }
                    EventDefinition::Link {
                        id,
                        name,
                        sources,
                        target,
                        ..
                    } => {
                        if !matches!(
                            n.kind,
                            NodeKind::IntermediateCatchEvent | NodeKind::IntermediateThrowEvent
                        ) {
                            return fail(
                                "Link definition requires an intermediate catch or throw event",
                            );
                        }
                        if owned && let Some(id) = id {
                            add(ids, id)?;
                        }
                        short(name)?;
                        if (n.kind == NodeKind::IntermediateCatchEvent && target.is_some())
                            || (n.kind == NodeKind::IntermediateThrowEvent && !sources.is_empty())
                        {
                            return fail("Invalid link definition direction");
                        }
                        let mut unique = HashSet::new();
                        for (reference, expected) in sources
                            .iter()
                            .map(|r| (r, NodeKind::IntermediateThrowEvent))
                            .chain(target.iter().map(|r| (r, NodeKind::IntermediateCatchEvent)))
                        {
                            if !unique.insert(reference) {
                                return fail("Duplicate link reference");
                            }
                            let valid = p.nodes.iter().any(|other| {
                                if other.kind != expected { return false; }
                                let Some(NodeDetails::Event { definitions, definition_refs, .. }) = &other.details else { return false; };
                                let matches_link = |definition: &EventDefinition| matches!(definition,
                                    EventDefinition::Link { id: Some(other_id), name: other_name, .. }
                                    if reference.matches_identity(other_id, local_namespace) && other_name == name);
                                definitions.iter().any(matches_link) || definition_refs.iter().any(|use_ref|
                                    d.root_event_definitions.iter().any(|definition|
                                        definition.id().is_some_and(|id| use_ref.matches_identity(id, local_namespace)) && matches_link(definition)))
                            });
                            if !valid && !reference.is_external(local_namespace) {
                                return fail("Invalid or cross-scope link reference");
                            }
                        }
                    }
                    EventDefinition::Cancel { id, .. } => {
                        if owned && let Some(id) = id {
                            add(ids, id)?;
                        }
                        let valid = match n.kind {
                            NodeKind::BoundaryEvent => {
                                cancel != Some(false)
                                    && attached.is_some_and(|target| {
                                        target.is_external(local_namespace)
                                            || p.nodes.iter().any(|other| {
                                                target.matches_identity(&other.id, local_namespace)
                                                    && other.kind == NodeKind::Transaction
                                            })
                                    })
                            }
                            NodeKind::EndEvent => crate::queries::find_node(d, &p.id)
                                .is_some_and(|owner| owner.kind == NodeKind::Transaction),
                            _ => false,
                        };
                        if !valid {
                            return fail(
                                "Cancel definition requires an interrupting transaction boundary or transaction end event",
                            );
                        }
                    }
                    EventDefinition::Compensate {
                        id, activity_ref, ..
                    } => {
                        if owned && let Some(id) = id {
                            add(ids, id)?;
                        }
                        let valid_kind = matches!(
                            n.kind,
                            NodeKind::BoundaryEvent
                                | NodeKind::IntermediateThrowEvent
                                | NodeKind::EndEvent
                        ) || (n.kind == NodeKind::StartEvent
                            && crate::queries::find_node(d, &p.id).is_some_and(|owner| {
                                owner
                                    .activity
                                    .as_ref()
                                    .is_some_and(|a| a.triggered_by_event == Some(true))
                            }));
                        if !valid_kind {
                            return fail("Invalid compensation event context");
                        }
                        if activity_ref.as_ref().is_some_and(|reference| {
                            !matches!(
                                n.kind,
                                NodeKind::IntermediateThrowEvent | NodeKind::EndEvent
                            ) || !p.nodes.iter().any(|other| {
                                reference.matches_identity(&other.id, local_namespace)
                                    && other.kind.is_activity()
                            }) && !reference.is_external(local_namespace)
                        }) {
                            return fail("Invalid compensation activity reference");
                        }
                    }
                    EventDefinition::Terminate { id, .. }
                        if matches!(n.kind, NodeKind::EndEvent) =>
                    {
                        if owned && let Some(id) = id {
                            add(ids, id)?;
                        }
                    }
                    _ => return fail("unsupported event definition"),
                }
            }
        } else if matches!(n.kind, NodeKind::BoundaryEvent) {
            return fail("boundary event requires attachment");
        }
        Ok(())
    }

    pub(super) fn register_node(&mut self, p: &Scope<'a>, n: &'a Node) -> Result<(), String> {
        let node_process = &mut self.node_process;
        let kinds = &mut self.kinds;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        shape_ids.insert(n.id.selector());
        element_scope.insert(n.id.selector(), p.id.clone());
        node_process.insert(n.id.selector(), p.id.clone());
        kinds.insert(n.id.selector(), n.kind);
        Ok(())
    }

    pub(super) fn check_scope_lanes(&mut self, p: &Scope<'a>) -> Result<(), String> {
        let ids = &mut self.ids;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        for l in p.lanes {
            add(ids, &l.id)?;
            if let Some(id) = &l.child_lane_set_id {
                add(ids, id)?;
            }
            if let Some(name) = &l.name {
                short(name)?;
            }
            shape_ids.insert(l.id.selector());
            element_scope.insert(l.id.selector(), p.id.clone());
        }
        Ok(())
    }

    pub(super) fn check_scope_flows(&mut self, p: &Scope<'a>) -> Result<(), String> {
        let ids = &mut self.ids;
        let flow_ids = &mut self.flow_ids;
        let element_scope = &mut self.element_scope;
        for f in p.flows {
            if has_flow_node_refs(&f.flow_metadata) {
                return fail("incoming/outgoing references require a flow node");
            }
            if f.flow_metadata.is_some() && f.kind != FlowKind::Sequence {
                return fail("Flow-element metadata requires a sequence flow");
            }
            if f.association_direction.is_some() && f.kind != FlowKind::Association {
                return fail("association direction requires association");
            }
            add(ids, &f.id)?;
            flow_ids.insert(f.id.selector());
            element_scope.insert(f.id.selector(), p.id.clone());
            if let Some(name) = &f.name {
                short(name)?;
            }
        }
        Ok(())
    }

    pub(super) fn check_choreography_identity(
        &mut self,
        choreography: &'a crate::bpmn_collaboration::Choreography,
    ) -> Result<(), String> {
        let ids = &mut self.ids;
        add(ids, &choreography.id)?;
        if let Some(name) = &choreography.name {
            short(name)?;
        }
        Ok(())
    }

    pub(super) fn check_choreography_conversation_tree(
        &self,
        choreography: &'a crate::bpmn_collaboration::Choreography,
    ) -> Result<(), String> {
        let conversation_parents: HashMap<_, _> = choreography
            .conversation_nodes
            .iter()
            .map(|n| (n.id.selector(), n.parent.clone()))
            .collect();
        acyclic_selectors(&conversation_parents, "conversation")?;
        Ok(())
    }

    pub(super) fn check_choreography_conversation_nodes(
        &mut self,
        choreography: &'a crate::bpmn_collaboration::Choreography,
    ) -> Result<(), String> {
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        for node in &choreography.conversation_nodes {
            if node.call.is_some()
                && node.kind != crate::bpmn_collaboration::ConversationKind::CallConversation
            {
                return fail("Call conversation metadata requires a call conversation");
            }
            if node.parent.as_ref().is_some_and(|parent| {
                !choreography.conversation_nodes.iter().any(|n| {
                    &n.id == parent
                        && n.kind == crate::bpmn_collaboration::ConversationKind::SubConversation
                })
            }) {
                return fail("invalid parent conversation");
            }
            let mut seen = HashSet::new();
            for participant in &node.participant_refs {
                if !seen.insert(participant)
                    || (!participant.is_external(local_namespace)
                        && !choreography
                            .participants
                            .iter()
                            .any(|p| participant.matches_identity(&p.id, local_namespace)))
                {
                    return fail("invalid conversation participant");
                }
            }
            add(ids, &node.id)?;
            shape_ids.insert(node.id.selector());
            element_scope.insert(node.id.selector(), choreography.id.selector());
        }
        Ok(())
    }

    pub(super) fn check_choreography_conversation_links(
        &mut self,
        choreography: &'a crate::bpmn_collaboration::Choreography,
    ) -> Result<(), String> {
        let ids = &mut self.ids;
        let flow_ids = &mut self.flow_ids;
        let element_scope = &mut self.element_scope;
        for link in &choreography.conversation_links {
            add(ids, &link.id)?;
            flow_ids.insert(link.id.selector());
            element_scope.insert(link.id.selector(), choreography.id.selector());
        }
        Ok(())
    }

    pub(super) fn check_choreography_participants(
        &mut self,
        choreography: &'a crate::bpmn_collaboration::Choreography,
    ) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        for participant in &choreography.participants {
            add(ids, &participant.id)?;
            if let Some(name) = &participant.name {
                short(name)?;
            }
            shape_ids.insert(participant.id.selector());
            element_scope.insert(participant.id.selector(), choreography.id.selector());
            if participant.process.as_ref().is_some_and(|id| {
                !id.is_external(local_namespace)
                    && !d
                        .processes
                        .iter()
                        .any(|p| id.matches_identity(&p.id, local_namespace))
            }) {
                return fail("missing choreography participant process");
            }
        }
        Ok(())
    }

    pub(super) fn check_choreography_message_flows(
        &mut self,
        choreography: &'a crate::bpmn_collaboration::Choreography,
    ) -> Result<(), String> {
        let ids = &mut self.ids;
        let flow_ids = &mut self.flow_ids;
        let element_scope = &mut self.element_scope;
        let reference_index = &self.reference_index;
        let endpoint =|reference: &Option<crate::bpmn_activity::ModelReference>| -> Option<crate::ids::ElementSelector> {
            reference.as_ref().and_then(|reference| match reference_index.resolve(reference) { crate::reference_resolution::ReferenceResolution::Resolved(value) => Some(value), _ => None })
        };
        for flow in &choreography.message_flows {
            if flow.flow_metadata.is_some() {
                return fail("Flow-element metadata requires a sequence flow");
            }
            add(ids, &flow.id)?;
            if let Some(name) = &flow.name {
                short(name)?;
            }
            flow_ids.insert(flow.id.selector());
            element_scope.insert(flow.id.selector(), choreography.id.selector());
            let source = endpoint(&flow.source);
            let target = endpoint(&flow.target);
            if flow.kind != FlowKind::Message
                || flow.condition.is_some()
                || (source.is_some() && source == target)
                || source
                    .as_ref()
                    .is_some_and(|id| !choreography.participants.iter().any(|p| &p.id == id))
                || target
                    .as_ref()
                    .is_some_and(|id| !choreography.participants.iter().any(|p| &p.id == id))
            {
                return fail("invalid choreography message flow");
            }
        }
        Ok(())
    }

    pub(super) fn check_choreography_graph_mode(
        &self,
        choreography: &'a crate::bpmn_collaboration::Choreography,
    ) -> Result<(), String> {
        if choreography.graph.is_some()
            && (!choreography.activities.is_empty() || !choreography.sequence_flows.is_empty())
        {
            return fail("mixed choreography graph representations");
        }
        Ok(())
    }

    pub(super) fn check_choreography_activities(
        &mut self,
        choreography: &'a crate::bpmn_collaboration::Choreography,
        target_namespace: Option<&str>,
    ) -> Result<(), String> {
        let ids = &mut self.ids;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        for activity in &choreography.activities {
            if activity
                .flow_metadata
                .as_ref()
                .is_some_and(|m| m.is_immediate.is_some())
            {
                return fail("isImmediate requires a sequence flow");
            }
            add(ids, &activity.id)?;
            if let Some(name) = &activity.name {
                short(name)?;
            }
            shape_ids.insert(activity.id.selector());
            element_scope.insert(activity.id.selector(), choreography.id.selector());
            let mut unique = HashSet::new();
            for participant in &activity.participant_refs {
                if !unique.insert(participant)
                    || (!participant.is_external(target_namespace)
                        && !choreography
                            .participants
                            .iter()
                            .any(|p| participant.matches_identity(&p.id, target_namespace)))
                {
                    return fail("invalid choreography participant");
                }
            }
            if activity
                .initiating_participant
                .as_ref()
                .is_some_and(|r| !unique.contains(r) && !r.is_external(target_namespace))
            {
                return fail("invalid initiating participant");
            }
            let mut messages = HashSet::new();
            for flow in &activity.message_flow_refs {
                if !messages.insert(flow)
                    || (!flow.is_external(target_namespace)
                        && !choreography
                            .message_flows
                            .iter()
                            .any(|f| flow.matches_identity(&f.id, target_namespace)))
                {
                    return fail("invalid choreography message reference");
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_choreography_sequence_flows(
        &mut self,
        choreography: &'a crate::bpmn_collaboration::Choreography,
    ) -> Result<(), String> {
        let ids = &mut self.ids;
        let flow_ids = &mut self.flow_ids;
        let element_scope = &mut self.element_scope;
        let reference_index = &self.reference_index;
        let endpoint =|reference: &Option<crate::bpmn_activity::ModelReference>| -> Option<crate::ids::ElementSelector> {
            reference.as_ref().and_then(|reference| match reference_index.resolve(reference) { crate::reference_resolution::ReferenceResolution::Resolved(value) => Some(value), _ => None })
        };
        for flow in &choreography.sequence_flows {
            if has_flow_node_refs(&flow.flow_metadata) {
                return fail("incoming/outgoing references require a flow node");
            }
            add(ids, &flow.id)?;
            if let Some(name) = &flow.name {
                short(name)?;
            }
            flow_ids.insert(flow.id.selector());
            element_scope.insert(flow.id.selector(), choreography.id.selector());
            if flow.kind != FlowKind::Sequence
                || matches!(
                    &flow.source,
                    Some(crate::bpmn_activity::ModelReference::Qualified(_))
                )
                || matches!(
                    &flow.target,
                    Some(crate::bpmn_activity::ModelReference::Qualified(_))
                )
                || endpoint(&flow.source)
                    .as_ref()
                    .is_some_and(|id| !choreography.activities.iter().any(|a| &a.id == id))
                || endpoint(&flow.target)
                    .as_ref()
                    .is_some_and(|id| !choreography.activities.iter().any(|a| &a.id == id))
            {
                return fail("invalid choreography sequence flow");
            }
        }
        Ok(())
    }

    pub(super) fn check_choreography_graphs(&mut self) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let flow_ids = &mut self.flow_ids;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        let mut count = self.count;
        let reference_index = &self.reference_index;
        let endpoint =|reference: &Option<crate::bpmn_activity::ModelReference>| -> Option<crate::ids::ElementSelector> {
            reference.as_ref().and_then(|reference| match reference_index.resolve(reference) { crate::reference_resolution::ReferenceResolution::Resolved(value) => Some(value), _ => None })
        };
        // Graph membership and identity are scoped; never resolve an endpoint by local name alone.
        for root in &d.choreographies {
            use crate::bpmn_collaboration::{
                ChoreographyActivityDetails, ChoreographyFlowElement, CollaborationArtifact,
            };
            let Some(graph) = &root.graph else { continue };
            let mut pending = vec![(graph, root.id.selector(), 0usize)];
            while let Some((graph, owner, depth)) = pending.pop() {
                if depth > 32 {
                    return fail("choreography depth limit exceeded");
                }
                count = count
                    .saturating_add(graph.flow_elements.len())
                    .saturating_add(graph.data_associations.len());
                if count > MAX_DOCUMENT_ELEMENTS {
                    return fail("choreography element limit exceeded");
                }
                let nodes: HashSet<_> = graph
                    .flow_elements
                    .iter()
                    .filter_map(|v| match v {
                        ChoreographyFlowElement::Activity(a) => Some(a.id.selector()),
                        ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => {
                            Some(n.id.selector())
                        }
                        ChoreographyFlowElement::SequenceFlow(_) => None,
                    })
                    .collect();
                for association in &graph.data_associations {
                    use crate::bpmn_data::DataAssociationKind;
                    if let Some(id) = &association.id {
                        add(ids, id)?;
                    }
                    if association.assignments.len() > MAX_COLLECTION_ITEMS {
                        return fail("Data assignment limit exceeded");
                    }
                    for assignment in &association.assignments {
                        if let Some(id) = &assignment.id {
                            add(ids, id)?;
                        }
                    }
                    let Some(event) = graph.flow_elements.iter().find_map(|item| match item {
                        ChoreographyFlowElement::Event(node) if node.id == association.owner => {
                            Some(node)
                        }
                        _ => None,
                    }) else {
                        return fail("invalid choreography data association owner");
                    };
                    let Some(crate::bpmn_model::NodeDetails::Event {
                        io: Some(io),
                        properties,
                        ..
                    }) = &event.details
                    else {
                        return fail("data association requires compatible owner ports");
                    };
                    use crate::bpmn_model::EventIo;
                    let (inputs, outputs): (
                        &[crate::bpmn_data::DataPort],
                        &[crate::bpmn_data::DataPort],
                    ) = match io {
                        EventIo::Catch { outputs, .. } => (&[], outputs),
                        EventIo::Throw { inputs, .. } => (inputs, &[]),
                    };
                    let local = |id: &crate::ids::ElementSelector| {
                        properties
                            .iter()
                            .any(|p| p.id.as_ref().is_some_and(|identity| identity == id))
                            || d.data_stores.iter().any(|store| {
                                store.id.as_ref().is_some_and(|identity| identity == id)
                            })
                    };
                    let mut unique = HashSet::new();
                    if association.sources.iter().any(|id| !unique.insert(id)) {
                        return fail("invalid data association sources");
                    }
                    let valid = match association.kind {
                        DataAssociationKind::Input => {
                            inputs.iter().any(|p| {
                                p.id.as_ref()
                                    .is_some_and(|identity| identity == &association.target)
                            }) && association.sources.iter().all(local)
                        }
                        DataAssociationKind::Output => {
                            local(&association.target)
                                && association.sources.iter().all(|id| {
                                    outputs.iter().any(|p| {
                                        p.id.as_ref().is_some_and(|identity| identity == id)
                                    })
                                })
                        }
                    };
                    if !valid {
                        return fail("invalid data association ports");
                    }
                }
                for element in &graph.flow_elements {
                    match element {
                        ChoreographyFlowElement::Activity(a) => {
                            add(ids, &a.id)?;
                            shape_ids.insert(a.id.selector());
                            element_scope.insert(a.id.selector(), owner.clone());
                            if let Some(name) = &a.name {
                                short(name)?;
                            }
                            if a.details.is_some() && !a.message_flow_refs.is_empty() {
                                return fail(
                                    "non-task choreography activity has task message references",
                                );
                            }
                            if let Some(ChoreographyActivityDetails::Sub { graph, artifacts }) =
                                &a.details
                            {
                                count = count.saturating_add(artifacts.len());
                                if count > MAX_DOCUMENT_ELEMENTS {
                                    return fail("choreography element limit exceeded");
                                }
                                for artifact in artifacts {
                                    match artifact {
                                        CollaborationArtifact::Group(g) => {
                                            add(ids, &g.id)?;
                                            shape_ids.insert(g.id.selector());
                                            element_scope.insert(g.id.selector(), a.id.selector());
                                        }
                                        CollaborationArtifact::TextAnnotation(n) => {
                                            add(ids, &n.id)?;
                                            shape_ids.insert(n.id.selector());
                                            element_scope.insert(n.id.selector(), a.id.selector());
                                        }
                                        CollaborationArtifact::Association(f) => {
                                            add(ids, &f.id)?;
                                            flow_ids.insert(f.id.selector());
                                            element_scope.insert(f.id.selector(), a.id.selector());
                                        }
                                    }
                                }
                                pending.push((graph, a.id.selector(), depth + 1));
                            }
                        }
                        ChoreographyFlowElement::Event(n) | ChoreographyFlowElement::Gateway(n) => {
                            let event = matches!(
                                n.kind,
                                NodeKind::StartEvent
                                    | NodeKind::EndEvent
                                    | NodeKind::IntermediateCatchEvent
                                    | NodeKind::IntermediateThrowEvent
                                    | NodeKind::BoundaryEvent
                            );
                            let gateway = matches!(
                                n.kind,
                                NodeKind::ExclusiveGateway
                                    | NodeKind::InclusiveGateway
                                    | NodeKind::ParallelGateway
                                    | NodeKind::ComplexGateway
                                    | NodeKind::EventBasedGateway
                            );
                            if (matches!(element, ChoreographyFlowElement::Event(_)) && !event)
                                || (matches!(element, ChoreographyFlowElement::Gateway(_))
                                    && !gateway)
                            {
                                return fail("invalid choreography flow node kind");
                            }
                            if event {
                                use crate::bpmn_model::NodeDetails;
                                let attachment = match &n.details {
                                    Some(NodeDetails::BoundaryEvent { attached_to, .. }) => {
                                        Some(attached_to)
                                    }
                                    Some(NodeDetails::Event { attached_to, .. }) => {
                                        attached_to.as_ref()
                                    }
                                    None => None,
                                    _ => return fail("invalid choreography event details"),
                                };
                                if n.kind == NodeKind::BoundaryEvent {
                                    if attachment.is_some_and(|attachment| !attachment.is_external(local_namespace) && !graph.flow_elements.iter().any(|element| matches!(element, ChoreographyFlowElement::Activity(activity) if attachment.matches_identity(&activity.id, local_namespace)))) {
                                        return fail("invalid choreography boundary attachment");
                                    }
                                } else if attachment.is_some() {
                                    return fail("attachment only allowed on boundary events");
                                }
                            } else if !matches!(
                                n.details,
                                None | Some(crate::bpmn_model::NodeDetails::Gateway { .. })
                            ) {
                                return fail("invalid choreography gateway details");
                            }
                            add(ids, &n.id)?;
                            if let Some(crate::bpmn_model::NodeDetails::Event {
                                io: Some(io),
                                ..
                            }) = &n.details
                            {
                                let catch = matches!(
                                    n.kind,
                                    NodeKind::StartEvent
                                        | NodeKind::IntermediateCatchEvent
                                        | NodeKind::BoundaryEvent
                                );
                                if !matches!(
                                    (catch, io),
                                    (true, crate::bpmn_model::EventIo::Catch { .. })
                                        | (false, crate::bpmn_model::EventIo::Throw { .. })
                                ) {
                                    return fail("Event I/O direction does not match event kind");
                                }
                            }
                            if let Some(crate::bpmn_model::NodeDetails::Event {
                                definitions, ..
                            }) = &n.details
                            {
                                if definitions.len() > MAX_COLLECTION_ITEMS {
                                    return fail("Event definition limit exceeded");
                                }
                                for definition in definitions {
                                    validate_event_payload(d, definition, local_namespace)?;
                                    if let Some(id) = definition.id() {
                                        add(ids, id)?;
                                    }
                                }
                            }
                            shape_ids.insert(n.id.selector());
                            element_scope.insert(n.id.selector(), owner.clone());
                        }
                        ChoreographyFlowElement::SequenceFlow(f) => {
                            add(ids, &f.id)?;
                            flow_ids.insert(f.id.selector());
                            element_scope.insert(f.id.selector(), owner.clone());
                            if f.kind != FlowKind::Sequence
                                || matches!(
                                    f.source,
                                    Some(crate::bpmn_activity::ModelReference::Qualified(_))
                                )
                                || matches!(
                                    f.target,
                                    Some(crate::bpmn_activity::ModelReference::Qualified(_))
                                )
                                || endpoint(&f.source)
                                    .as_ref()
                                    .is_some_and(|id| !nodes.contains(id))
                                || endpoint(&f.target)
                                    .as_ref()
                                    .is_some_and(|id| !nodes.contains(id))
                            {
                                return fail("invalid choreography sequence flow");
                            }
                        }
                    }
                }
            }
        }
        self.count = count;
        Ok(())
    }

    pub(super) fn check_collaborations(&mut self) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let flow_ids = &mut self.flow_ids;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        let reference_index = &self.reference_index;
        let endpoint =|reference: &Option<crate::bpmn_activity::ModelReference>| -> Option<crate::ids::ElementSelector> {
            reference.as_ref().and_then(|reference| match reference_index.resolve(reference) { crate::reference_resolution::ReferenceResolution::Resolved(value) => Some(value), _ => None })
        };
        for c in d.collaboration.iter().chain(&d.additional_collaborations) {
            add(ids, &c.id)?;
            if let Some(name) = &c.name {
                definitions_text(name)?;
            }

            for p in &c.participants {
                add(ids, &p.id)?;
                shape_ids.insert(p.id.selector());
                // Older native projects placed their own participant on a process plane.
                // Retain those projects without accepting participants from other processes.
                if let Some(process) = endpoint(&p.process) {
                    element_scope.insert(p.id.selector(), process);
                }
                if let Some(name) = &p.name {
                    short(name)?;
                }

                if p.process.as_ref().is_some_and(|id| {
                    !id.is_external(local_namespace)
                        && !d
                            .processes
                            .iter()
                            .any(|x| id.matches_identity(&x.id, local_namespace))
                }) {
                    return fail("missing participant process");
                }
            }
            for f in &c.flows {
                if f.flow_metadata.is_some() && f.kind != FlowKind::Sequence {
                    return fail("Flow-element metadata requires a sequence flow");
                }
                add(ids, &f.id)?;
                flow_ids.insert(f.id.selector());
                if let Some(name) = &f.name {
                    short(name)?;
                }
            }
            let parents: HashMap<_, _> = c
                .conversation_nodes
                .iter()
                .map(|n| (n.id.selector(), n.parent.clone()))
                .collect();
            acyclic_selectors(&parents, "conversation")?;
            for node in &c.conversation_nodes {
                if node.call.is_some()
                    && node.kind != crate::bpmn_collaboration::ConversationKind::CallConversation
                {
                    return fail("Call conversation metadata requires a call conversation");
                }
                add(ids, &node.id)?;
                if let Some(name) = &node.name {
                    short(name)?;
                }
                shape_ids.insert(node.id.selector());
                if node.parent.as_ref().is_some_and(|parent| {
                    !c.conversation_nodes.iter().any(|n| {
                        &n.id == parent
                            && matches!(
                                n.kind,
                                crate::bpmn_collaboration::ConversationKind::SubConversation
                            )
                    })
                }) {
                    return fail("invalid parent conversation");
                }
                let mut unique = HashSet::new();
                for participant in &node.participant_refs {
                    if !unique.insert(participant)
                        || (!participant.is_external(local_namespace)
                            && !c
                                .participants
                                .iter()
                                .any(|p| participant.matches_identity(&p.id, local_namespace)))
                    {
                        return fail("invalid conversation participant");
                    }
                }
            }
            for link in &c.conversation_links {
                add(ids, &link.id)?;
                if let Some(name) = &link.name {
                    short(name)?;
                }
                flow_ids.insert(link.id.selector());
                let participant =
                    |id: &crate::ids::ElementSelector| c.participants.iter().any(|p| &p.id == id);
                let conversation = |id: &crate::ids::ElementSelector| {
                    c.conversation_nodes.iter().any(|n| &n.id == id)
                };
                if let (Some(source), Some(target)) =
                    (endpoint(&link.source), endpoint(&link.target))
                    && !((participant(&source) && conversation(&target))
                        || (conversation(&source) && participant(&target)))
                {
                    return fail("invalid conversation link endpoints");
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_scope_lane_hierarchy(&mut self, p: &Scope<'a>) -> Result<(), String> {
        let node_process = &mut self.node_process;
        let lanes: HashMap<_, _> = p
            .lanes
            .iter()
            .map(|l| (l.id.selector(), l.parent.clone()))
            .collect();
        acyclic_selectors(&lanes, "lane")?;
        let mut members = HashSet::new();
        for l in p.lanes {
            for m in &l.members {
                if node_process.get(m) != Some(&p.id) || !members.insert(m) {
                    return fail("invalid or duplicate lane membership");
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_scope_association_endpoints(
        &self,
        p: &Scope<'a>,
        scopes_by_id: &HashMap<crate::ids::ElementSelector, &Scope<'a>>,
    ) -> Result<(), String> {
        let d = self.d;
        for association in p.associations {
            use crate::bpmn_data::DataAssociationKind;
            let Some(owner) = p.nodes.iter().find(|n| n.id == association.owner) else {
                return fail("invalid data association owner");
            };
            let (inputs, outputs): (&[crate::bpmn_data::DataPort], &[crate::bpmn_data::DataPort]) =
                if owner.kind.is_activity() {
                    // Modelers such as bpmn.io write no ioSpecification: inputs then target an
                    // activity property and outputs carry no source (both XSD-valid).
                    owner
                        .io_specification
                        .as_ref()
                        .map_or((&[], &[]), |io| (&io.inputs, &io.outputs))
                } else if let Some(crate::bpmn_model::NodeDetails::Event {
                    io: Some(event_io),
                    ..
                }) = &owner.details
                {
                    match event_io {
                        crate::bpmn_model::EventIo::Catch { outputs, .. } => (&[], outputs),
                        crate::bpmn_model::EventIo::Throw { inputs, .. } => (inputs, &[]),
                    }
                } else {
                    return fail("data association requires compatible owner ports");
                };
            let local = |id: &crate::ids::ElementSelector, input_source: bool| {
                if let Some(crate::bpmn_model::NodeDetails::Event { properties, .. }) =
                    &owner.details
                    && properties
                        .iter()
                        .any(|property| property.id.as_ref().is_some_and(|identity| identity == id))
                {
                    return true;
                }
                if d.data_stores
                    .iter()
                    .any(|store| store.id.as_ref().is_some_and(|identity| identity == id))
                {
                    return true;
                }
                let mut current = Some(p.id.clone());
                while let Some(scope_id) = current {
                    let Some(scope) = scopes_by_id.get(&scope_id) else {
                        break;
                    };
                    if scope
                        .objects
                        .iter()
                        .any(|o| o.id.as_ref().is_some_and(|identity| identity == id))
                        || scope.nodes.iter().any(|n| {
                            n.id == *id
                                && matches!(
                                    n.kind,
                                    NodeKind::DataObjectReference | NodeKind::DataStoreReference
                                )
                        })
                        || d.processes
                            .iter()
                            .find(|process| process.id == scope_id)
                            .and_then(|process| process.io_specification.as_ref())
                            .is_some_and(|io| {
                                let ports = if input_source {
                                    &io.inputs
                                } else {
                                    &io.outputs
                                };
                                ports.iter().any(|port| {
                                    port.id.as_ref().is_some_and(|identity| identity == id)
                                })
                            })
                    {
                        return true;
                    }
                    current = scope.parent.clone();
                }
                false
            };
            let mut unique = HashSet::new();
            if association.sources.iter().any(|id| !unique.insert(id)) {
                return fail("invalid data association sources");
            }
            let valid = match association.kind {
                DataAssociationKind::Input => {
                    (inputs.iter().any(|port| {
                        port.id
                            .as_ref()
                            .is_some_and(|identity| identity == &association.target)
                    }) || owner.activity.as_ref().is_some_and(|activity| {
                        activity.properties.iter().any(|property| {
                            property
                                .id
                                .as_ref()
                                .is_some_and(|identity| identity == &association.target)
                        })
                    })) && association.sources.iter().all(|id| local(id, true))
                }
                DataAssociationKind::Output => {
                    local(&association.target, false)
                        && association.sources.iter().all(|id| {
                            outputs
                                .iter()
                                .any(|port| port.id.as_ref().is_some_and(|identity| identity == id))
                        })
                }
            };
            if !valid {
                return fail("invalid data association ports");
            }
        }
        Ok(())
    }

    pub(super) fn check_scope_flow_endpoints(&mut self, p: &Scope<'a>) -> Result<(), String> {
        let node_process = &mut self.node_process;
        let kinds = &mut self.kinds;
        let group_ids = &mut self.group_ids;
        let reference_index = &self.reference_index;
        let endpoint =|reference: &Option<crate::bpmn_activity::ModelReference>| -> Option<crate::ids::ElementSelector> {
            reference.as_ref().and_then(|reference| match reference_index.resolve(reference) { crate::reference_resolution::ReferenceResolution::Resolved(value) => Some(value), _ => None })
        };
        for f in p.flows {
            if f.kind != FlowKind::Sequence && f.condition.is_some() {
                return fail("condition requires sequence flow");
            }
            if f.kind == FlowKind::Sequence
                && (matches!(
                    &f.source,
                    Some(crate::bpmn_activity::ModelReference::Qualified(_))
                ) || matches!(
                    &f.target,
                    Some(crate::bpmn_activity::ModelReference::Qualified(_))
                ))
            {
                return fail("Sequence flow endpoints require IDREF selectors");
            }
            let source = endpoint(&f.source);
            let target = endpoint(&f.target);
            if f.kind == FlowKind::Message {
                return fail("invalid process flow endpoints");
            }
            if let (Some(source), Some(target)) = (&source, &target) {
                if f.kind == FlowKind::Association
                    && !group_ids.contains(source)
                    && !group_ids.contains(target)
                    && !matches!(
                        kinds.get(source),
                        Some(NodeKind::TextAnnotation | NodeKind::DataObjectReference)
                    )
                    && !matches!(
                        kinds.get(target),
                        Some(NodeKind::TextAnnotation | NodeKind::DataObjectReference)
                    )
                    // A compensation association links a boundary event to its handler.
                    && !(kinds.get(source) == Some(&NodeKind::BoundaryEvent)
                        && kinds.get(target).is_some_and(|kind| kind.is_activity()))
                {
                    return fail("association requires an artifact endpoint");
                }
                // Association refs are any BaseElement: a node of any scope may be an end.
                let association_endpoint = |reference: &crate::ids::ElementSelector| {
                    node_process.contains_key(reference)
                        || p.flows
                            .iter()
                            .any(|flow| flow.id == *reference && flow.kind == FlowKind::Sequence)
                };
                if (f.kind == FlowKind::Association
                    && (!association_endpoint(source) || !association_endpoint(target)))
                    || (f.kind != FlowKind::Association
                        && (node_process.get(source) != Some(&p.id)
                            || node_process.get(target) != Some(&p.id)))
                {
                    return fail("invalid process flow endpoints");
                }
                if f.kind == FlowKind::Sequence
                    && (!kinds.get(source).is_some_and(|kind| {
                        is_flow_node(*kind) && !matches!(kind, NodeKind::EndEvent)
                    }) || !kinds.get(target).is_some_and(|kind| {
                        is_flow_node(*kind) && !matches!(kind, NodeKind::StartEvent)
                    }))
                {
                    return fail("invalid sequence endpoint");
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_collaboration_flows(
        &mut self,
        scopes_by_id: &HashMap<crate::ids::ElementSelector, &Scope<'a>>,
    ) -> Result<(), String> {
        let d = self.d;
        let node_process = &mut self.node_process;
        let kinds = &mut self.kinds;
        let reference_index = &self.reference_index;
        let endpoint =|reference: &Option<crate::bpmn_activity::ModelReference>| -> Option<crate::ids::ElementSelector> {
            reference.as_ref().and_then(|reference| match reference_index.resolve(reference) { crate::reference_resolution::ReferenceResolution::Resolved(value) => Some(value), _ => None })
        };
        for c in d.collaboration.iter().chain(&d.additional_collaborations) {
            let participant_for = |id: &crate::ids::ElementSelector,
                                   is_source: bool|
             -> Option<crate::ids::ElementSelector> {
                if let Some(p) = c.participants.iter().find(|p| &p.id == id) {
                    return Some(p.id.selector());
                }
                // BPMN 2.0.2 §9.4: message flows leave activities and throwing
                // events, and reach activities and catching events.
                if !kinds.get(id).is_some_and(|kind| {
                    kind.is_activity()
                        || if is_source {
                            matches!(kind, NodeKind::IntermediateThrowEvent | NodeKind::EndEvent)
                        } else {
                            matches!(
                                kind,
                                NodeKind::StartEvent
                                    | NodeKind::IntermediateCatchEvent
                                    | NodeKind::BoundaryEvent
                            )
                        }
                }) {
                    return None;
                }
                // Interaction nodes nested in subprocesses belong to the pool of their root process.
                let mut process = node_process.get(id)?.clone();
                for _ in 0..=32 {
                    match scopes_by_id
                        .get(&process)
                        .and_then(|scope| scope.parent.clone())
                    {
                        Some(parent) => process = parent,
                        None => break,
                    }
                }
                let mut ps = c
                    .participants
                    .iter()
                    .filter(|p| endpoint(&p.process).as_ref() == Some(&process));
                let first = ps.next()?;
                if ps.next().is_some() {
                    None
                } else {
                    Some(first.id.selector())
                }
            };
            for f in &c.flows {
                if f.flow_metadata.is_some() && f.kind != FlowKind::Sequence {
                    return fail("Flow-element metadata requires a sequence flow");
                }
                let source = endpoint(&f.source);
                let target = endpoint(&f.target);
                if f.condition.is_some()
                    || f.kind != FlowKind::Message
                    || matches!((&source, &target), (Some(source), Some(target)) if participant_for(source, true).is_none() || participant_for(target, false).is_none() || participant_for(source, true) == participant_for(target, false))
                {
                    return fail("invalid message flow endpoints");
                }
            }
        }
        Ok(())
    }

    pub(super) fn check_diagram_header(&mut self, diagram: &'a Diagram) -> Result<(), String> {
        let ids = &mut self.ids;
        add(ids, &diagram.id)?;
        if let Some(name) = &diagram.name {
            definitions_text(name)?;
        }
        add(ids, &diagram.plane_id)?;
        di_metadata(&diagram.plane_metadata)?;
        if let Some(order) = &diagram.plane_order {
            let expected: HashSet<_> = diagram
                .shapes
                .iter()
                .map(|s| s.id.selector())
                .chain(diagram.edges.iter().map(|e| e.id.selector()))
                .collect();
            let actual: HashSet<_> = order.iter().cloned().collect();
            if actual.len() != order.len() || actual != expected {
                return fail("Plane order must contain each representation exactly once");
            }
        }
        for style in &diagram.label_styles {
            if let Some(id) = &style.id {
                add(ids, id)?;
            }
        }
        Ok(())
    }

    pub(super) fn check_diagram_plane(
        &self,
        diagram: &'a Diagram,
        scopes: &[Scope<'a>],
    ) -> Result<
        (
            Vec<crate::queries::ChoreographyScope<'a>>,
            HashSet<crate::ids::ElementSelector>,
        ),
        String,
    > {
        let d = self.d;
        let choreography_scopes = crate::queries::choreography_scopes(d);
        if !scopes.iter().any(|p| p.id == diagram.plane)
            && !choreography_scopes
                .iter()
                .any(|scope| scope.owner == diagram.plane)
            && !d.collaborations().any(|c| c.id == diagram.plane)
        {
            return fail("invalid diagram plane");
        }
        let mut contained = HashSet::new();
        if choreography_scopes
            .iter()
            .any(|scope| scope.owner == diagram.plane)
        {
            contained.insert(diagram.plane.clone());
            // Scopes are ordered parent-before-child. Only the selected scope and
            // descendants belong to this plane, never siblings or ancestors.
            for scope in &choreography_scopes {
                if scope
                    .parent
                    .as_ref()
                    .is_some_and(|parent| contained.contains(parent))
                {
                    contained.insert(scope.owner.clone());
                }
            }
        }
        if let Some(start) = scopes.iter().find(|p| p.id == diagram.plane) {
            let mut pending = vec![start];
            while let Some(scope) = pending.pop() {
                contained.insert(scope.id.clone());
                for node in scope.nodes {
                    if matches!(
                        node.details,
                        Some(crate::bpmn_model::NodeDetails::SubProcess { .. })
                    ) && let Some(child) = scopes.iter().find(|s| s.id == node.id.selector())
                    {
                        pending.push(child);
                    }
                }
            }
        }
        Ok((choreography_scopes, contained))
    }

    pub(super) fn check_diagram_shapes(
        &mut self,
        diagram: &'a Diagram,
        choreography_scopes: &[crate::queries::ChoreographyScope<'a>],
        contained: &HashSet<crate::ids::ElementSelector>,
    ) -> Result<(), String> {
        let d = self.d;
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let shape_ids = &mut self.shape_ids;
        let element_scope = &mut self.element_scope;
        let local =
            |name: &crate::bpmn_model::QNameRef| local_namespace == Some(name.namespace.as_str());
        let check_label = |label: &Option<Label>| -> Result<(), String> {
            if let Some(label) = label
                && let Some(crate::model::DiStyleRef::Local(target)) = &label.style_ref
                && !diagram
                    .label_styles
                    .iter()
                    .any(|style| style.id.as_ref().is_some_and(|id| id == target))
            {
                return fail("Label style reference is outside its diagram");
            }
            if let Some(label) = label
                && let Some(crate::model::DiStyleRef::Qualified(target)) = &label.style_ref
            {
                check_qname(target)?;
                if local(target)
                    && !diagram.label_styles.iter().any(|style| {
                        style.id.as_ref().and_then(|id| id.xml_id()) == Some(&target.local)
                    })
                {
                    return fail("Label style reference is outside its diagram");
                }
            }
            Ok(())
        };
        for s in &diagram.shapes {
            di_metadata(&s.di_metadata)?;
            let mut scoped_participant_band = false;
            if let Some(reference) = &s.choreography_activity_shape {
                let owner = match reference {
                    crate::model::DiReference::Local(selector) => {
                        diagram.shapes.iter().find(|shape| shape.id == *selector)
                    }
                    crate::model::DiReference::Qualified(qname) => {
                        check_qname(qname)?;
                        if local(qname) {
                            diagram
                                .shapes
                                .iter()
                                .find(|shape| shape.id.xml_id() == Some(&qname.local))
                        } else {
                            None
                        }
                    }
                };
                let external = matches!(reference, crate::model::DiReference::Qualified(qname) if !local(qname));
                if let Some(shape) = owner {
                    scoped_participant_band = choreography_scopes.iter().any(|scope| {
                        contained.contains(&scope.owner)
                            && scope.activities.iter().any(|activity| {
                                activity.id == shape.element
                                    && scope.root.participants.iter().any(|participant| {
                                        participant.id == s.element
                                            && activity.participant_refs.iter().any(|reference| {
                                                reference.matches_identity(
                                                    &participant.id,
                                                    local_namespace,
                                                )
                                            })
                                    })
                            })
                    });
                }
                if !external && !scoped_participant_band {
                    return fail(
                        "Participant band must reference a participant of a choreography activity shape in its diagram",
                    );
                }
            }
            add(ids, &s.id)?;
            let global_store = d
                .data_stores
                .iter()
                .any(|store| store.id.as_ref().is_some_and(|id| id == &s.element));
            if !global_store
                && !scoped_participant_band
                && !contained.is_empty()
                && !element_scope
                    .get(&s.element)
                    .is_some_and(|scope| contained.contains(scope))
            {
                return Err(format!("shape {} is outside diagram scope", s.element));
            }
            if !shape_ids.contains(&s.element) {
                return fail("invalid shape reference");
            }
            bounds(&s.bounds)?;
            label(&s.label)?;
            check_label(&s.label)?;
            if let Some(id) = s.label.as_ref().and_then(|l| l.id.as_ref()) {
                add(ids, id)?;
            }
        }
        Ok(())
    }

    pub(super) fn check_diagram_edges(
        &mut self,
        diagram: &'a Diagram,
        choreography_scopes: &[crate::queries::ChoreographyScope<'a>],
        contained: &HashSet<crate::ids::ElementSelector>,
    ) -> Result<(), String> {
        let local_namespace = self.local_namespace;
        let ids = &mut self.ids;
        let flow_ids = &mut self.flow_ids;
        let element_scope = &mut self.element_scope;
        let local =
            |name: &crate::bpmn_model::QNameRef| local_namespace == Some(name.namespace.as_str());
        let check_label = |label: &Option<Label>| -> Result<(), String> {
            if let Some(label) = label
                && let Some(crate::model::DiStyleRef::Local(target)) = &label.style_ref
                && !diagram
                    .label_styles
                    .iter()
                    .any(|style| style.id.as_ref().is_some_and(|id| id == target))
            {
                return fail("Label style reference is outside its diagram");
            }
            if let Some(label) = label
                && let Some(crate::model::DiStyleRef::Qualified(target)) = &label.style_ref
            {
                check_qname(target)?;
                if local(target)
                    && !diagram.label_styles.iter().any(|style| {
                        style.id.as_ref().and_then(|id| id.xml_id()) == Some(&target.local)
                    })
                {
                    return fail("Label style reference is outside its diagram");
                }
            }
            Ok(())
        };
        let representations: HashSet<_> = diagram
            .shapes
            .iter()
            .map(|shape| shape.id.selector())
            .chain(diagram.edges.iter().map(|edge| edge.id.selector()))
            .collect();
        for e in &diagram.edges {
            di_metadata(&e.di_metadata)?;
            add(ids, &e.id)?;
            if e.source_element
                .iter()
                .chain(e.target_element.iter())
                .any(|reference| !representations.contains(reference))
            {
                return fail("Edge endpoint references a representation outside its diagram");
            }
            let scoped_message_flow = choreography_scopes.iter().any(|scope| {
                contained.contains(&scope.owner)
                    && scope.root.message_flows.iter().any(|flow| {
                        flow.id == e.element
                            && scope.activities.iter().any(|activity| {
                                activity.message_flow_refs.iter().any(|reference| {
                                    reference.matches_identity(&flow.id, local_namespace)
                                })
                            })
                    })
            });
            if !scoped_message_flow
                && !contained.is_empty()
                && !element_scope
                    .get(&e.element)
                    .is_some_and(|scope| contained.contains(scope))
            {
                return fail("edge outside diagram scope");
            }
            if !flow_ids.contains(&e.element) || e.waypoints.len() < 2 {
                return fail("invalid edge reference or route");
            }
            for p in &e.waypoints {
                if !p.x.is_finite()
                    || !p.y.is_finite()
                    || p.x.abs() > MAX_COORDINATE
                    || p.y.abs() > MAX_COORDINATE
                {
                    return fail("invalid waypoint");
                }
            }
            label(&e.label)?;
            check_label(&e.label)?;
            if let Some(id) = e.label.as_ref().and_then(|l| l.id.as_ref()) {
                add(ids, id)?;
            }
        }
        Ok(())
    }

    pub(super) fn check_retained_extensions(&mut self) -> Result<(), String> {
        let d = self.d;
        let ids = &mut self.ids;
        if let Capability::RetainedExtensions { extensions, .. } = &d.capability {
            let semantics = semantic_ids(d);
            for e in extensions {
                if !semantics.contains(&e.owner) && !ids.iter().any(|id| id.selector() == e.owner) {
                    return fail("missing extension owner");
                }
            }
        }
        Ok(())
    }
}

pub(super) fn check_metadata(
    d: &BpmnDocument,
    collaboration: &crate::collaboration_read::CollaborationRead<'_>,
    activity: &crate::activity_read::ActivityRead<'_>,
) -> Result<(), String> {
    let mut metadata_error = Ok(());
    for docs in &collaboration.docs {
        crate::bpmn_metadata::validate_set(docs)?;
    }
    for extras in &collaboration.extras {
        crate::bpmn_metadata::validate_extras(extras)?;
    }
    for base in &collaboration.bases {
        crate::bpmn_metadata::validate_base(base)?;
    }
    for base in &activity.bases {
        crate::bpmn_metadata::validate_base(base)?;
    }
    crate::metadata_read::visit_base_metadata(d, |_, value| {
        if let Some(value) = value {
            metadata_error = metadata_error
                .clone()
                .and_then(|_| crate::bpmn_metadata::validate_base(value));
        }
    });
    crate::metadata_read::visit_legacy_metadata(d, |_, docs, extras| {
        metadata_error = metadata_error
            .clone()
            .and_then(|_| crate::bpmn_metadata::validate_set(docs));
        if let Some(extras) = extras {
            metadata_error = metadata_error
                .clone()
                .and_then(|_| crate::bpmn_metadata::validate_extras(extras));
        }
    });
    metadata_error?;
    Ok(())
}

pub(super) fn check_element_limit(
    d: &BpmnDocument,
    identities: &[&crate::ids::ElementIdentity],
    collaboration: &crate::collaboration_read::CollaborationRead<'_>,
    activity: &crate::activity_read::ActivityRead<'_>,
) -> Result<(), String> {
    let anonymous_multiplicities = d
        .collaboration
        .iter()
        .flat_map(|c| &c.participants)
        .chain(d.choreographies.iter().flat_map(|c| &c.participants))
        .filter(|p| p.multiplicity.as_ref().is_some_and(|m| m.id.is_none()))
        .count();
    if identities
        .len()
        .saturating_add(anonymous_multiplicities)
        .saturating_add(collaboration.anonymous)
        .saturating_add(activity.anonymous_owners)
        > 10_000
    {
        return fail("document element limit exceeded");
    }
    Ok(())
}

pub(super) fn check_root_order(d: &BpmnDocument) -> Result<(), String> {
    if let Some(order) = &d.root_order {
        let roots = d.root_element_identities();
        if roots.iter().any(|id| id.is_none()) {
            return fail(
                "Root order requires explicit native identity promotion for anonymous roots",
            );
        }
        let expected: HashSet<_> = roots
            .into_iter()
            .flatten()
            .map(|id| id.selector())
            .collect();
        let actual: HashSet<_> = order.iter().cloned().collect();
        if actual.len() != order.len() || actual != expected {
            return fail("Root order must contain each modeled root exactly once");
        }
    }
    Ok(())
}

pub(super) fn check_unique_identities(
    identities: Vec<&crate::ids::ElementIdentity>,
) -> Result<(), String> {
    let mut native_ids = HashSet::new();
    let mut xml_ids = HashSet::new();
    for identity in identities {
        if let crate::ids::ElementIdentity::Native { native_id, .. } = identity
            && !native_ids.insert(native_id)
        {
            return fail("duplicate native element ID");
        }
        if let Some(xml_id) = identity.xml_id()
            && !xml_ids.insert(xml_id)
        {
            return fail("duplicate XML ID");
        }
    }
    Ok(())
}

pub(super) fn check_definitions_metadata(d: &BpmnDocument) -> Result<(), String> {
    if let Some(metadata) = &d.definitions {
        definitions_text(&metadata.target_namespace)?;
        if !metadata.namespaces.is_empty() || !metadata.foreign_attributes.is_empty() {
            crate::bpmn_xml::validate_foreign_attributes(
                &metadata.foreign_attributes,
                &metadata.namespaces,
            )?;
        }
        if metadata.imports.len() > MAX_COLLECTION_ITEMS {
            return fail("Definitions import limit exceeded");
        }
        for import in &metadata.imports {
            definitions_text(&import.namespace)?;
            definitions_text(&import.location)?;
            definitions_text(&import.import_type)?;
        }
        for value in [
            &metadata.name,
            &metadata.expression_language,
            &metadata.type_language,
            &metadata.exporter,
            &metadata.exporter_version,
        ]
        .into_iter()
        .flatten()
        {
            definitions_text(value)?;
        }
    }
    Ok(())
}

pub(super) fn check_qnames(
    d: &BpmnDocument,
    activity_qnames: &[&crate::bpmn_model::QNameRef],
) -> Result<(), String> {
    for name in activity_qnames {
        check_qname(name)?;
    }
    for name in crate::collaboration_read::collaboration_qnames(d) {
        check_qname(name)?;
    }
    Ok(())
}

pub(super) fn check_flow_message_refs(d: &BpmnDocument) -> Result<(), String> {
    for scope in crate::queries::scopes(d) {
        for flow in scope.flows {
            check_flow_message(flow)?;
        }
    }
    for collaboration in d.collaborations() {
        for flow in &collaboration.flows {
            check_flow_message(flow)?;
        }
    }
    for choreography in &d.choreographies {
        for flow in choreography
            .message_flows
            .iter()
            .chain(&choreography.sequence_flows)
        {
            check_flow_message(flow)?;
        }
    }
    Ok(())
}
