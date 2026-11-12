//! Normative activity/loop/event/flow-element checks (BPMN 2.0.2 §§8.4.7, 10.3, 10.5).
//! Every message cites the clause it enforces; these are advisory warnings like the rest of validation.
use crate::{
    bpmn_activity::MultiInstanceFlowCondition,
    bpmn_model::{
        EventDefinition, Expression, ExpressionData, ExpressionKind, LoopCharacteristics,
        NodeDetails, QNameRef,
    },
    ids::ElementSelector,
    model::{BpmnDocument, Flow, FlowKind, Node, NodeKind},
};

pub struct FlowIssue {
    pub owner: ElementSelector,
    pub code: &'static str,
    pub message: String,
}

struct Ctx<'a> {
    doc: &'a BpmnDocument,
    ns: Option<&'a str>,
    out: Vec<FlowIssue>,
}

pub fn flow_issues(doc: &BpmnDocument) -> Vec<FlowIssue> {
    let ns = doc
        .definitions
        .as_ref()
        .map(|d| d.target_namespace.as_str())
        .or_else(|| {
            doc.provenance
                .is_none()
                .then_some(crate::model::GENERATED_TARGET_NAMESPACE)
        });
    let mut cx = Ctx {
        doc,
        ns,
        out: Vec::new(),
    };
    for process in &doc.processes {
        walk(
            &mut cx,
            &process.nodes,
            &process.flows,
            process.is_executable == Some(true),
            0,
        );
    }
    lane_issues(doc, &mut cx.out);
    label_style_issues(doc, &mut cx.out);
    root_issues(doc, cx.ns, &mut cx.out);
    cx.out
}

/// Lane partition checks, BPMN 2.0.2 §10.8 Table 10.135 (p. 307): `partitionElementRef` references a
/// BaseElement, and "All Lanes in a single LaneSet MUST define partition element of the same type".
fn lane_issues(doc: &BpmnDocument, out: &mut Vec<FlowIssue>) {
    use crate::reference_resolution::{ReferenceIndex, ReferenceResolution};
    let index = ReferenceIndex::new(doc);
    let values: Vec<ElementSelector> = doc
        .categories
        .iter()
        .flat_map(|c| &c.values)
        .map(|v| v.id.selector())
        .collect();
    for process in &doc.processes {
        // (lane set parent, partition type) of every partitioned lane.
        let mut kinds: Vec<(Option<&ElementSelector>, &'static str)> = Vec::new();
        for lane in &process.lanes {
            let kind = if lane.partition_element.is_some() {
                Some("CategoryValue")
            } else if let Some(r) = &lane.partition_element_ref {
                match index.resolve(r) {
                    ReferenceResolution::MissingLocal => {
                        out.push(FlowIssue {
                            owner: lane.id.selector(),
                            code: "danglingPartitionElementRef",
                            message: "Lane partitionElementRef does not resolve to a BaseElement; BPMN 2.0.2 §10.8, Table 10.135 (p. 307).".into(),
                        });
                        None
                    }
                    ReferenceResolution::Resolved(id) => Some(if values.contains(&id) {
                        "CategoryValue"
                    } else if process.nodes.iter().any(|n| n.id.selector() == id) {
                        "FlowNode"
                    } else {
                        "BaseElement"
                    }),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(kind) = kind {
                kinds.push((lane.parent.as_ref(), kind));
            }
        }
        for lane in &process.lanes {
            let mine = kinds.iter().filter(|(p, _)| *p == lane.parent.as_ref());
            if lane.partition_element.is_some() || lane.partition_element_ref.is_some() {
                let mut distinct: Vec<_> = mine.map(|(_, k)| *k).collect();
                distinct.dedup();
                distinct.sort_unstable();
                distinct.dedup();
                if distinct.len() > 1 {
                    out.push(FlowIssue {
                        owner: lane.id.selector(),
                        code: "mixedLanePartitionTypes",
                        message: "Lanes of one LaneSet define partition elements of different types; BPMN 2.0.2 §10.8: all Lanes in a LaneSet MUST define partition element of the same type (p. 307).".into(),
                    });
                }
            }
        }
    }
}

/// Process and Collaboration root constraints.
fn root_issues(doc: &BpmnDocument, ns: Option<&str>, out: &mut Vec<FlowIssue>) {
    use crate::bpmn_activity::ProcessType;
    // Table 10.1 (p. 146): isExecutable "MAY not be true for public Processes".
    for process in &doc.processes {
        if process.is_executable == Some(true)
            && process
                .activity_metadata
                .as_ref()
                .and_then(|m| m.process_type)
                == Some(ProcessType::Public)
        {
            out.push(FlowIssue {
                owner: process.id.selector(),
                code: "publicProcessExecutable",
                message: "A public Process must not be executable (isExecutable=true); BPMN 2.0.2 §10.1 Table 10.1.".into(),
            });
        }
    }
    // Table 8.48 (p. 93): Message itemRef is an ItemDefinition.
    for resource in &doc.event_resources {
        if let crate::bpmn_model::EventResource::Message {
            id,
            item_ref: Some(item),
            ..
        } = resource
            && ns == Some(item.namespace.as_str())
            && !doc
                .catalog
                .item_definitions
                .iter()
                .any(|d| d.id.as_ref().and_then(|i| i.xml_id()) == Some(&item.local))
        {
            out.push(FlowIssue {
                owner: id.selector(),
                code: "messageItemRefUnresolved",
                message: "Message itemRef does not resolve to an ItemDefinition; BPMN 2.0.2 §8.4.9 Table 8.48.".into(),
            });
        }
    }
    // §9.2 (p. 109): "If a Collaboration contains a Choreography, then the value of the isClosed
    // attribute MUST be the same in both."
    let closed = |e: &Option<crate::bpmn_collaboration::CollaborationExtras>| {
        e.as_ref().and_then(|e| e.is_closed).unwrap_or(false)
    };
    if let Some(collaboration) = &doc.collaboration
        && let Some(extras) = &collaboration.extras
    {
        for choreography in &doc.choreographies {
            if extras
                .choreography_refs
                .iter()
                .any(|r| r.matches_identity(&choreography.id, ns))
                && closed(&collaboration.extras) != closed(&choreography.extras)
            {
                out.push(FlowIssue {
                    owner: collaboration.id.selector(),
                    code: "collaborationChoreographyClosedMismatch",
                    message: "A Collaboration and its Choreography MUST have the same isClosed value; BPMN 2.0.2 §9.2 (p. 109).".into(),
                });
            }
        }
    }
}

/// BPMNLabel, §12.2.3.7 (p. 376): "The referenced style is owned by the diagram that nests the label";
/// BPMNDiagram §12.2.3.1 (p. 370): label styles are owned by the diagram.
fn label_style_issues(doc: &BpmnDocument, out: &mut Vec<FlowIssue>) {
    use crate::model::DiStyleRef;
    for diagram in &doc.diagrams {
        let owned: Vec<ElementSelector> = diagram
            .label_styles
            .iter()
            .filter_map(|s| s.id.as_ref().map(|id| id.selector()))
            .collect();
        let labelled = diagram
            .shapes
            .iter()
            .map(|s| (s.id.selector(), &s.label))
            .chain(diagram.edges.iter().map(|e| (e.id.selector(), &e.label)));
        for (owner, label) in labelled {
            if let Some(DiStyleRef::Local(style)) =
                label.as_ref().and_then(|l| l.style_ref.as_ref())
                && !owned.contains(style)
            {
                out.push(FlowIssue {
                    owner,
                    code: "danglingLabelStyle",
                    message: "BPMNLabel style is not a BPMNLabelStyle owned by the nesting diagram; BPMN 2.0.2 §12.2.3.7 (p. 376).".into(),
                });
            }
        }
    }
}

fn push(cx: &mut Ctx, owner: ElementSelector, code: &'static str, message: &str) {
    cx.out.push(FlowIssue {
        owner,
        code,
        message: message.into(),
    });
}

fn is_integer(text: &str) -> bool {
    let t = text.trim();
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

fn is_mime_type(text: &str) -> bool {
    let token = |t: &str| {
        !t.is_empty()
            && t.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$&^_.+-".contains(&b))
    };
    // Parameters (";charset=utf-8") are allowed after the type/subtype pair.
    let essence = text.split(';').next().unwrap_or("").trim();
    essence
        .split_once('/')
        .is_some_and(|(t, s)| token(t) && token(s))
}

fn walk(cx: &mut Ctx, nodes: &[Node], flows: &[Flow], executable: bool, depth: usize) {
    if depth >= 32 {
        return;
    }
    let ns = cx.ns;
    let find = |reference: &Option<crate::bpmn_activity::ModelReference>| {
        reference
            .as_ref()
            .and_then(|r| nodes.iter().find(|n| r.matches_identity(&n.id, ns)))
    };
    // Table 10.3 (p. 150) and §10.7.1 (p. 302): a Compensation Activity is outside the normal flow.
    let mut pending = Vec::new();
    for flow in flows.iter().filter(|f| f.kind == FlowKind::Sequence) {
        let (Some(s), Some(t)) = (find(&flow.source), find(&flow.target)) else {
            continue;
        };
        if !s.kind.is_activity() || !t.kind.is_activity() {
            continue;
        }
        let comp = |n: &Node| n.activity.as_ref().and_then(|a| a.is_for_compensation) == Some(true);
        if comp(s) != comp(t) {
            pending.push(if comp(s) { s } else { t }.id.selector());
        }
    }
    for owner in pending {
        push(
            cx,
            owner,
            "compensationActivityInNormalFlow",
            "An Activity with isForCompensation=true is activated only by a Compensation Event and lies outside the normal Sequence Flow of the Process; BPMN 2.0.2 §10.3.1 Table 10.3, §10.7.1.",
        );
    }
    // Table 8.44 (p. 86): auditing and monitoring can only be defined for a Process.
    for (owner, meta) in nodes
        .iter()
        .map(|n| (n.id.selector(), &n.flow_metadata))
        .chain(flows.iter().map(|f| (f.id.selector(), &f.flow_metadata)))
    {
        if meta
            .as_ref()
            .is_some_and(|m| m.auditing.is_some() || m.monitoring.is_some())
        {
            push(
                cx,
                owner,
                "flowElementAuditMonitorMisplaced",
                "Auditing and monitoring can only be defined for a Process, not a FlowElement; BPMN 2.0.2 §8.4.7 Table 8.44.",
            );
        }
    }
    for node in nodes {
        node_checks(cx, node, executable);
        // Table 8.24 (p. 70): textFormat "MUST follow the mime-type format".
        if let Some(NodeDetails::TextAnnotation {
            text_format: Some(format),
            ..
        }) = &node.details
            && !is_mime_type(format)
        {
            push(
                cx,
                node.id.selector(),
                "invalidTextFormat",
                "TextAnnotation textFormat MUST follow the mime-type format (type/subtype); BPMN 2.0.2 §8.3.1 Table 8.24.",
            );
        }
        if let Some(NodeDetails::SubProcess { content }) = &node.details {
            walk(cx, &content.nodes, &content.flows, executable, depth + 1);
        }
    }
}

fn node_checks(cx: &mut Ctx, node: &Node, executable: bool) {
    let owner = node.id.selector();
    // Table 10.8 (p. 157): ##unspecified, ##WebService, or a URI identifying another technology.
    if let Some(implementation) = node
        .activity
        .as_ref()
        .and_then(|a| a.implementation.as_ref())
        && (implementation.chars().any(char::is_whitespace)
            || (implementation.starts_with("##")
                && !matches!(implementation.as_str(), "##unspecified" | "##WebService")))
    {
        push(
            cx,
            owner.clone(),
            "activityImplementationInvalid",
            "implementation must be \"##unspecified\", \"##WebService\" or a URI; BPMN 2.0.2 §10.3.3, Table 10.8.",
        );
    }
    match &node.loop_characteristics {
        // Table 10.28 (p. 190): loopMaximum is an integer.
        Some(LoopCharacteristics::Standard {
            maximum: Some(m), ..
        }) if !is_integer(m) => push(
            cx,
            owner.clone(),
            "standardLoopMaximumNotInteger",
            "StandardLoopCharacteristics loopMaximum must be an integer; BPMN 2.0.2 §10.3.7, Table 10.28.",
        ),
        Some(LoopCharacteristics::MultiInstance {
            id,
            behavior,
            one_behavior_event_ref,
            none_behavior_event_ref,
            input_data_item,
            output_data_item,
            ..
        }) => {
            let loop_owner = id.as_ref().map_or_else(|| owner.clone(), |i| i.selector());
            // Table 10.29 (p. 192): oneEvent/noneEvent are thrown only when behavior is One/None (default All).
            let effective = behavior.unwrap_or(MultiInstanceFlowCondition::All);
            for (present, wanted, code, message) in [
                (
                    one_behavior_event_ref.is_some(),
                    MultiInstanceFlowCondition::One,
                    "multiInstanceOneEventBehaviorMismatch",
                    "oneBehaviorEventRef is thrown only when behavior is One; BPMN 2.0.2 §10.3.8, Table 10.29.",
                ),
                (
                    none_behavior_event_ref.is_some(),
                    MultiInstanceFlowCondition::None,
                    "multiInstanceNoneEventBehaviorMismatch",
                    "noneBehaviorEventRef is thrown only when behavior is None; BPMN 2.0.2 §10.3.8, Table 10.29.",
                ),
            ] {
                if present && effective != wanted {
                    push(cx, loop_owner.clone(), code, message);
                }
            }
            // Table 10.29 (p. 191): inputDataItem/outputDataItem hold the scalar of the loop collection.
            for (port, code, message) in [
                (
                    input_data_item,
                    "multiInstanceInputItemNotScalar",
                    "inputDataItem is the single item of the loopDataInput collection, so its type MUST be scalar; BPMN 2.0.2 §10.3.8, Table 10.29.",
                ),
                (
                    output_data_item,
                    "multiInstanceOutputItemNotScalar",
                    "outputDataItem is the single item of the loopDataOutput collection, so its type MUST be scalar; BPMN 2.0.2 §10.3.8, Table 10.29.",
                ),
            ] {
                let Some(port) = port else { continue };
                let referenced = port
                    .item_aware
                    .as_ref()
                    .and_then(|a| a.item_subject_ref.as_ref())
                    .and_then(|q| item_collection(cx, q));
                if port.is_collection == Some(true) || referenced == Some(true) {
                    let at = port
                        .id
                        .as_ref()
                        .map_or_else(|| loop_owner.clone(), |i| i.selector());
                    push(cx, at, code, message);
                }
            }
        }
        _ => {}
    }
    if let Some(NodeDetails::Event {
        definitions,
        definition_refs,
        cancel_activity,
        ..
    }) = &node.details
    {
        let mut all: Vec<&EventDefinition> = definitions.iter().collect();
        all.extend(cx.doc.root_event_definitions.iter().filter(|d| {
            d.id().is_some_and(|id| {
                definition_refs
                    .iter()
                    .any(|r| r.matches_identity(id, cx.ns))
            })
        }));
        // Table 10.92 (p. 257): Error and Cancel boundary events always cancel the Activity.
        if node.kind == NodeKind::BoundaryEvent
            && *cancel_activity == Some(false)
            && all.iter().any(|d| {
                matches!(
                    d,
                    EventDefinition::Error { .. } | EventDefinition::Cancel { .. }
                )
            })
        {
            push(
                cx,
                owner.clone(),
                "boundaryErrorCancelNotInterrupting",
                "cancelActivity must be true for Error and Cancel boundary events; BPMN 2.0.2 §10.5.4, Tables 10.91-10.92.",
            );
        }
        // Table 10.95 (p. 264): an executable Process needs a FormalExpression for a Conditional trigger.
        if executable {
            for d in &all {
                if let EventDefinition::Conditional {
                    id,
                    expression:
                        Expression::Structured(ExpressionData {
                            kind: ExpressionKind::Base { .. },
                            ..
                        }),
                    ..
                } = d
                {
                    let at = id.as_ref().map_or_else(|| owner.clone(), |i| i.selector());
                    push(
                        cx,
                        at,
                        "conditionalEventNotFormalExpression",
                        "A Conditional EventDefinition in an executable Process MUST use a FormalExpression; BPMN 2.0.2 §10.5.5, Table 10.95.",
                    );
                }
            }
        }
    }
}

fn item_collection(cx: &Ctx, q: &QNameRef) -> Option<bool> {
    if cx.ns != Some(q.namespace.as_str()) {
        return None;
    }
    cx.doc
        .catalog
        .item_definitions
        .iter()
        .find(|d| d.id.as_ref().and_then(|i| i.xml_id()) == Some(&q.local))
        .map(|d| d.is_collection == Some(true))
}
