//! Normative data-family checks (BPMN 2.0.2 §10.4.1, Tables 10.51-10.63).
//! Every message cites the clause it enforces; these are advisory warnings like the rest of validation.
use crate::{
    bpmn_data::{DataPort, DataStoreRef, IoSet, IoSpecification, ItemAwareMetadata},
    bpmn_model::{NodeDetails, QNameRef},
    ids::{ElementIdentity, ElementSelector},
    model::{BpmnDocument, NodeKind},
    queries,
};

pub struct DataIssue {
    pub owner: ElementSelector,
    pub code: &'static str,
    pub message: String,
}

pub fn data_issues(doc: &BpmnDocument) -> Vec<DataIssue> {
    let ns = doc
        .definitions
        .as_ref()
        .map(|d| d.target_namespace.as_str())
        .or_else(|| {
            doc.provenance
                .is_none()
                .then_some(crate::model::GENERATED_TARGET_NAMESPACE)
        });
    let mut out = Vec::new();
    let qmatch = |q: &QNameRef, id: &ElementIdentity| {
        ns == Some(q.namespace.as_str()) && id.xml_id() == Some(&q.local)
    };
    let external = |q: &QNameRef| ns != Some(q.namespace.as_str());
    // itemSubjectRef must resolve (Table 10.51); isCollection must equal the ItemDefinition's (Tables 10.52, 10.59, 10.60).
    let item = |out: &mut Vec<DataIssue>,
                owner: ElementSelector,
                aware: &Option<ItemAwareMetadata>,
                collection: Option<bool>,
                label: &str,
                table: &str| {
        let Some(reference) = aware.as_ref().and_then(|a| a.item_subject_ref.as_ref()) else {
            return;
        };
        if external(reference) {
            return;
        }
        match doc
            .catalog
            .item_definitions
            .iter()
            .find(|d| d.id.as_ref().is_some_and(|id| qmatch(reference, id)))
        {
            None => out.push(DataIssue {
                owner,
                code: "dataItemSubjectUnresolved",
                message: format!(
                    "{label} itemSubjectRef does not resolve to an ItemDefinition; BPMN 2.0.2 §10.4.1, Table 10.51."
                ),
            }),
            Some(definition) => {
                if collection.is_some()
                    && collection.unwrap_or(false) != definition.is_collection.unwrap_or(false)
                {
                    out.push(DataIssue {
                        owner,
                        code: "dataCollectionMismatch",
                        message: format!(
                            "{label} isCollection MUST equal the referenced ItemDefinition isCollection; BPMN 2.0.2 §10.4.1, {table}."
                        ),
                    });
                }
            }
        }
    };

    for store in &doc.data_stores {
        let owner = store
            .id
            .as_ref()
            .map_or_else(|| doc.definitions_id.selector(), |id| id.selector());
        item(
            &mut out,
            owner.clone(),
            &store.item_aware,
            None,
            "DataStore",
            "Table 10.55",
        );
        if store
            .capacity
            .as_deref()
            .is_some_and(|c| c.trim().parse::<i128>().is_err())
        {
            out.push(DataIssue {
                owner,
                code: "dataStoreCapacityInvalid",
                message: "DataStore capacity MUST be an integer; BPMN 2.0.2 §10.4.1, Table 10.55."
                    .into(),
            });
        }
    }

    let port_check = |out: &mut Vec<DataIssue>,
                      owner: &ElementSelector,
                      port: &DataPort,
                      label: &str,
                      table: &str| {
        item(
            out,
            port.id
                .as_ref()
                .map_or_else(|| owner.clone(), |i| i.selector()),
            &port.item_aware,
            port.is_collection,
            label,
            table,
        );
    };
    // `resolve_sets`: also resolve set members (activity I/O only; Process/GlobalTask sets are resolved by the reference scan).
    let io_check = |out: &mut Vec<DataIssue>,
                    owner: &ElementSelector,
                    io: &IoSpecification,
                    resolve_sets: bool| {
        for p in &io.inputs {
            port_check(out, owner, p, "DataInput", "Table 10.59");
        }
        for p in &io.outputs {
            port_check(out, owner, p, "DataOutput", "Table 10.60");
        }
        for (ports, sets, other, label, table, set_label) in [
            (
                &io.inputs,
                &io.input_sets,
                &io.output_sets,
                "DataInput",
                "Tables 10.59, 10.61",
                "InputSet",
            ),
            (
                &io.outputs,
                &io.output_sets,
                &io.input_sets,
                "DataOutput",
                "Tables 10.60, 10.62",
                "OutputSet",
            ),
        ] {
            for p in ports {
                let Some(id) = &p.id else { continue };
                if !sets.iter().any(|s| s.data_refs.contains(&id.selector())) {
                    out.push(DataIssue {
                        owner: id.selector(),
                        code: "dataPortWithoutSet",
                        message: format!(
                            "{label} MUST be referenced by at least one {set_label}; BPMN 2.0.2 §10.4.1, {table}."
                        ),
                    });
                }
            }
            for set in sets {
                let so = set
                    .id
                    .as_ref()
                    .map_or_else(|| owner.clone(), |i| i.selector());
                for (members, name) in [
                    (&set.optional_refs, "optional"),
                    (&set.while_executing_refs, "whileExecuting"),
                ] {
                    if members.iter().any(|m| !set.data_refs.contains(m)) {
                        out.push(DataIssue {
                            owner: so.clone(),
                            code: "ioSetMemberNotInDataRefs",
                            message: format!(
                                "{set_label} {name} references MUST NOT reference a port that is not listed in its data references; BPMN 2.0.2 §10.4.1, {table}."
                            ),
                        });
                    }
                }
                if resolve_sets {
                    let unresolved = |refs: &[ElementSelector], pool: &mut dyn Iterator<Item = Option<&ElementIdentity>>| {
                        let ids: Vec<_> = pool.flatten().collect();
                        refs.iter().any(|r| !ids.iter().any(|i| *i == r))
                    };
                    let data = set
                        .data_refs
                        .iter()
                        .chain(&set.optional_refs)
                        .chain(&set.while_executing_refs)
                        .cloned()
                        .collect::<Vec<_>>();
                    if unresolved(&data, &mut ports.iter().map(|p| p.id.as_ref())) {
                        out.push(DataIssue {
                            owner: so.clone(),
                            code: "ioSetPortRefUnresolved",
                            message: format!(
                                "{set_label} references a {label} that is not defined by the same InputOutputSpecification; BPMN 2.0.2 §10.4.1, {table}."
                            ),
                        });
                    }
                    if unresolved(
                        &set.counterpart_refs,
                        &mut other.iter().map(|s: &IoSet| s.id.as_ref()),
                    ) {
                        out.push(DataIssue {
                            owner: so,
                            code: "ioSetCounterpartUnresolved",
                            message: format!(
                                "{set_label} input/output rule references a set that is not defined by the same InputOutputSpecification; BPMN 2.0.2 §10.4.1, {table}."
                            ),
                        });
                    }
                }
            }
        }
    };
    for process in &doc.processes {
        if let Some(io) = &process.io_specification {
            io_check(&mut out, &process.id.selector(), io, false);
        }
    }
    for task in &doc.global_tasks {
        if let Some(io) = &task.io_specification {
            let owner = task
                .id
                .as_ref()
                .map_or_else(|| doc.definitions_id.selector(), |i| i.selector());
            io_check(&mut out, &owner, io, false);
        }
    }

    // Everything an association endpoint may legally name (Table 10.63: ItemAwareElement).
    let mut aware_ids: Vec<&ElementIdentity> = doc
        .data_stores
        .iter()
        .filter_map(|s| s.id.as_ref())
        .collect();
    for process in &doc.processes {
        if let Some(io) = &process.io_specification {
            aware_ids.extend(
                io.inputs
                    .iter()
                    .chain(&io.outputs)
                    .filter_map(|p| p.id.as_ref()),
            );
        }
        if let Some(m) = &process.activity_metadata {
            aware_ids.extend(m.properties.iter().filter_map(|p| p.id.as_ref()));
        }
    }
    let scopes = queries::scopes(doc);
    for scope in &scopes {
        aware_ids.extend(scope.data_objects.iter().filter_map(|o| o.id.as_ref()));
        for node in scope.nodes {
            if matches!(
                node.kind,
                NodeKind::DataObjectReference | NodeKind::DataStoreReference
            ) {
                aware_ids.push(&node.id);
            }
            if let Some(io) = &node.io_specification {
                aware_ids.extend(
                    io.inputs
                        .iter()
                        .chain(&io.outputs)
                        .filter_map(|p| p.id.as_ref()),
                );
            }
            if let Some(a) = &node.activity {
                aware_ids.extend(a.properties.iter().filter_map(|p| p.id.as_ref()));
            }
            if let Some(NodeDetails::Event { io, properties, .. }) = &node.details {
                {
                    use crate::bpmn_model::EventIo;
                    match io {
                        Some(EventIo::Catch { outputs, .. }) => {
                            aware_ids.extend(outputs.iter().filter_map(|p| p.id.as_ref()))
                        }
                        Some(EventIo::Throw { inputs, .. }) => {
                            aware_ids.extend(inputs.iter().filter_map(|p| p.id.as_ref()))
                        }
                        None => {}
                    }
                    aware_ids.extend(properties.iter().filter_map(|p| p.id.as_ref()));
                }
            }
            if let Some(crate::bpmn_model::LoopCharacteristics::MultiInstance {
                input_data_item,
                output_data_item,
                ..
            }) = &node.loop_characteristics
            {
                aware_ids.extend(
                    [input_data_item, output_data_item]
                        .into_iter()
                        .flatten()
                        .filter_map(|p| p.id.as_ref()),
                );
            }
        }
    }
    let process_inputs: Vec<&ElementIdentity> = doc
        .processes
        .iter()
        .filter_map(|p| p.io_specification.as_ref())
        .flat_map(|io| io.inputs.iter().filter_map(|p| p.id.as_ref()))
        .collect();
    let process_outputs: Vec<&ElementIdentity> = doc
        .processes
        .iter()
        .filter_map(|p| p.io_specification.as_ref())
        .flat_map(|io| io.outputs.iter().filter_map(|p| p.id.as_ref()))
        .collect();

    for scope in &scopes {
        for object in scope.data_objects {
            let owner = object
                .id
                .as_ref()
                .map_or_else(|| scope.id.selector(), |i| i.selector());
            item(
                &mut out,
                owner,
                &object.item_aware,
                object.is_collection,
                "DataObject",
                "Table 10.52",
            );
        }
        for node in scope.nodes {
            if node.kind == NodeKind::DataStoreReference {
                item(
                    &mut out,
                    node.id.selector(),
                    &node.item_aware,
                    None,
                    "DataStoreReference",
                    "Table 10.56",
                );
                if let Some(NodeDetails::DataStoreReference {
                    data_store_ref: Some(reference),
                }) = &node.details
                {
                    let resolved = match reference {
                        DataStoreRef::Legacy(sel) => doc
                            .data_stores
                            .iter()
                            .any(|s| s.id.as_ref().is_some_and(|i| i == sel)),
                        DataStoreRef::Qualified(q) => {
                            external(q)
                                || doc
                                    .data_stores
                                    .iter()
                                    .any(|s| s.id.as_ref().is_some_and(|i| qmatch(q, i)))
                        }
                    };
                    if !resolved {
                        out.push(DataIssue {
                            owner: node.id.selector(),
                            code: "dataStoreRefUnresolved",
                            message: "dataStoreRef does not resolve to a DataStore; BPMN 2.0.2 §10.4.1, Table 10.56.".into(),
                        });
                    }
                }
            }
            if let Some(io) = &node.io_specification {
                io_check(&mut out, &node.id.selector(), io, true);
            }
        }
        for association in scope.data_associations {
            let owner = association
                .id
                .as_ref()
                .map_or_else(|| scope.id.selector(), |i| i.selector());
            for (selector, what) in association
                .sources
                .iter()
                .map(|s| (s, "source"))
                .chain([(&association.target, "target")])
            {
                if !aware_ids.iter().any(|i| *i == selector) {
                    out.push(DataIssue {
                        owner: owner.clone(),
                        code: "dataAssociationEndpointUnresolved",
                        message: format!(
                            "Data association {what} MUST be an ItemAwareElement; BPMN 2.0.2 §10.4.1, Table 10.63."
                        ),
                    });
                }
            }
            // §10.4.1 DataInput/DataOutput: a top-level Process port is not an association target/source,
            // except for the Start/End Event exceptions of §10.4.2.
            let kind = scope
                .nodes
                .iter()
                .find(|n| n.id == association.owner)
                .map(|n| n.kind);
            if kind.is_some() {
                if kind != Some(NodeKind::StartEvent)
                    && process_inputs.iter().any(|i| **i == association.target)
                {
                    out.push(DataIssue {
                        owner: owner.clone(),
                        code: "dataAssociationProcessInputTarget",
                        message: "A Data Input directly contained by a Process MUST NOT be the target of a Data Association (only a Start Event may fill it); BPMN 2.0.2 §10.4.1, Table 10.59 text and §10.4.2.".into(),
                    });
                }
                if kind != Some(NodeKind::EndEvent)
                    && association
                        .sources
                        .iter()
                        .any(|s| process_outputs.iter().any(|i| *i == s))
                {
                    out.push(DataIssue {
                        owner,
                        code: "dataAssociationProcessOutputSource",
                        message: "A Data Output directly contained by a Process MUST NOT be the source of a Data Association (only an End Event may read it); BPMN 2.0.2 §10.4.1, Table 10.60 text and §10.4.2.".into(),
                    });
                }
            }
        }
    }
    out
}
