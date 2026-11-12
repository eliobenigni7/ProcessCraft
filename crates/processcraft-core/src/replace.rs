//! Replace (morph) an element into another BPMN type, re-implemented from the bpmn-js replace
//! option tables and replace behavior (not copied): [`replace_options`] lists what an element
//! can become, [`replace`] changes it in place keeping its ID, name, documentation, owner,
//! lanes, flows, and DI, and removes the flows the rules no longer allow. Kind-specific data the
//! new type cannot hold is refused, never dropped silently.
use crate::{
    bpmn_activity::{ActivityProperties, ModelReference},
    bpmn_model::{EventDefinition, EventGatewayType, EventIo, Expression, NodeDetails, TimerKind},
    commands::Mutation,
    geometry,
    ids::ElementSelector,
    model::*,
    modeling, queries, rules,
};
use serde::{Deserialize, Serialize};

/// Event definition type of an event replace target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    Message,
    Timer,
    Conditional,
    Signal,
    Error,
    Escalation,
    Compensate,
    Cancel,
    Link,
    Terminate,
}
impl Trigger {
    fn of(definition: &EventDefinition) -> Self {
        match definition {
            EventDefinition::Message { .. } => Self::Message,
            EventDefinition::Timer { .. } => Self::Timer,
            EventDefinition::Conditional { .. } => Self::Conditional,
            EventDefinition::Signal { .. } => Self::Signal,
            EventDefinition::Error { .. } => Self::Error,
            EventDefinition::Escalation { .. } => Self::Escalation,
            EventDefinition::Compensate { .. } => Self::Compensate,
            EventDefinition::Cancel { .. } => Self::Cancel,
            EventDefinition::Link { .. } => Self::Link,
            EventDefinition::Terminate { .. } => Self::Terminate,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Message => "Message",
            Self::Timer => "Timer",
            Self::Conditional => "Conditional",
            Self::Signal => "Signal",
            Self::Error => "Error",
            Self::Escalation => "Escalation",
            Self::Compensate => "Compensation",
            Self::Cancel => "Cancel",
            Self::Link => "Link",
            Self::Terminate => "Terminate",
        }
    }
    /// A fresh definition; timers get the palette default (duration PT1M).
    fn definition(self) -> EventDefinition {
        let (id, base) = (None, None);
        match self {
            Self::Message => EventDefinition::Message {
                id,
                base,
                message_ref: None,
                operation_ref: None,
            },
            Self::Timer => EventDefinition::Timer {
                id,
                base,
                kind: Some(TimerKind::Duration),
                expression: Some("PT1M".into()),
            },
            Self::Conditional => EventDefinition::Conditional {
                id,
                base,
                expression: "".into(),
            },
            Self::Signal => EventDefinition::Signal {
                id,
                base,
                signal_ref: None,
            },
            Self::Error => EventDefinition::Error {
                id,
                base,
                error_ref: None,
            },
            Self::Escalation => EventDefinition::Escalation {
                id,
                base,
                escalation_ref: None,
            },
            Self::Compensate => EventDefinition::Compensate {
                id,
                base,
                wait_for_completion: None,
                activity_ref: None,
            },
            Self::Cancel => EventDefinition::Cancel { id, base },
            Self::Link => EventDefinition::Link {
                id,
                base,
                name: String::new(),
                sources: Vec::new(),
                target: None,
            },
            Self::Terminate => EventDefinition::Terminate { id, base },
        }
    }
}
/// Whether replacing `definition` would lose authored data (references, expressions, IDs).
fn carries_data(definition: &EventDefinition) -> bool {
    let empty = |e: &Expression| e.plain_text().is_some_and(str::is_empty);
    definition.id().is_some()
        || definition.base().is_some()
        || match definition {
            EventDefinition::Message {
                message_ref,
                operation_ref,
                ..
            } => message_ref.is_some() || operation_ref.is_some(),
            EventDefinition::Signal { signal_ref, .. } => signal_ref.is_some(),
            EventDefinition::Error { error_ref, .. } => error_ref.is_some(),
            EventDefinition::Escalation { escalation_ref, .. } => escalation_ref.is_some(),
            EventDefinition::Conditional { expression, .. } => !empty(expression),
            EventDefinition::Link {
                name,
                sources,
                target,
                ..
            } => !name.is_empty() || !sources.is_empty() || target.is_some(),
            EventDefinition::Compensate {
                wait_for_completion,
                activity_ref,
                ..
            } => wait_for_completion.is_some() || activity_ref.is_some(),
            // The palette default timer counts as empty.
            EventDefinition::Timer {
                kind, expression, ..
            } => {
                !(kind.is_none() && expression.is_none()
                    || *kind == Some(TimerKind::Duration)
                        && expression.as_ref().and_then(Expression::plain_text) == Some("PT1M"))
            }
            EventDefinition::Cancel { .. } | EventDefinition::Terminate { .. } => false,
        }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// What an element can be replaced with (bpmn-js replace menu entries).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ReplaceTarget {
    /// A flow node, data reference type: `kind` plus the event definition type (`trigger`, none
    /// for a blank event), non-interrupting boundary/event-subprocess start, event subprocess
    /// (a subprocess triggered by event), and collapsed subprocess.
    Node {
        kind: NodeKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trigger: Option<Trigger>,
        #[serde(default, skip_serializing_if = "is_false")]
        non_interrupting: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        event_sub_process: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        collapsed: bool,
    },
    /// Plain sequence flow: no condition, not the source's default.
    SequenceFlow,
    /// The source's default flow, without condition.
    DefaultFlow,
    /// A sequence flow with an (empty) condition expression.
    ConditionalFlow,
}
fn node(kind: NodeKind) -> ReplaceTarget {
    ReplaceTarget::Node {
        kind,
        trigger: None,
        non_interrupting: false,
        event_sub_process: false,
        collapsed: false,
    }
}
fn event(kind: NodeKind, trigger: Trigger) -> ReplaceTarget {
    ReplaceTarget::Node {
        kind,
        trigger: Some(trigger),
        non_interrupting: false,
        event_sub_process: false,
        collapsed: false,
    }
}
fn non_interrupting(kind: NodeKind, trigger: Trigger) -> ReplaceTarget {
    ReplaceTarget::Node {
        kind,
        trigger: Some(trigger),
        non_interrupting: true,
        event_sub_process: false,
        collapsed: false,
    }
}
fn sub(kind: NodeKind, event_sub_process: bool, collapsed: bool) -> ReplaceTarget {
    ReplaceTarget::Node {
        kind,
        trigger: None,
        non_interrupting: false,
        event_sub_process,
        collapsed,
    }
}
fn is_sub(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::SubProcess | NodeKind::AdHocSubProcess | NodeKind::Transaction
    )
}

impl ReplaceTarget {
    /// Menu label, e.g. "Non-interrupting timer boundary event".
    pub fn label(&self) -> String {
        let Self::Node {
            kind,
            trigger,
            non_interrupting,
            event_sub_process,
            collapsed,
        } = self
        else {
            return match self {
                Self::DefaultFlow => "Default flow",
                Self::ConditionalFlow => "Conditional flow",
                _ => "Sequence flow",
            }
            .into();
        };
        let base = match kind {
            NodeKind::Task => "Task",
            NodeKind::UserTask => "User task",
            NodeKind::ManualTask => "Manual task",
            NodeKind::ServiceTask => "Service task",
            NodeKind::SendTask => "Send task",
            NodeKind::ReceiveTask => "Receive task",
            NodeKind::ScriptTask => "Script task",
            NodeKind::BusinessRuleTask => "Business rule task",
            NodeKind::CallActivity => "Call activity",
            NodeKind::SubProcess if *event_sub_process => "Event subprocess",
            NodeKind::SubProcess if *collapsed => "Collapsed subprocess",
            NodeKind::SubProcess => "Expanded subprocess",
            NodeKind::AdHocSubProcess if *collapsed => "Collapsed ad-hoc subprocess",
            NodeKind::AdHocSubProcess => "Expanded ad-hoc subprocess",
            NodeKind::Transaction => "Transaction",
            NodeKind::ExclusiveGateway => "Exclusive gateway",
            NodeKind::ParallelGateway => "Parallel gateway",
            NodeKind::InclusiveGateway => "Inclusive gateway",
            NodeKind::ComplexGateway => "Complex gateway",
            NodeKind::EventBasedGateway => "Event-based gateway",
            NodeKind::StartEvent => "start event",
            NodeKind::EndEvent => "end event",
            NodeKind::IntermediateCatchEvent if trigger.is_some() => "intermediate catch event",
            NodeKind::IntermediateThrowEvent if trigger.is_some() => "intermediate throw event",
            NodeKind::IntermediateCatchEvent | NodeKind::IntermediateThrowEvent => {
                "intermediate event"
            }
            NodeKind::BoundaryEvent => "boundary event",
            NodeKind::DataObjectReference => "Data object reference",
            NodeKind::DataStoreReference => "Data store reference",
            NodeKind::TextAnnotation => "Text annotation",
        };
        if !rules::is_event(*kind) {
            return base.into();
        }
        let typed = match trigger {
            Some(t) => format!("{} {base}", t.name()),
            None => {
                let mut chars = base.chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
        };
        if *non_interrupting {
            format!("Non-interrupting {}", typed.to_lowercase())
        } else {
            typed
        }
    }
}

fn endpoint(reference: &Option<ModelReference>) -> Option<&ElementSelector> {
    match reference {
        Some(ModelReference::Local(id)) => Some(id),
        _ => None,
    }
}
fn all_flows(doc: &BpmnDocument) -> impl Iterator<Item = &Flow> {
    queries::scopes(doc)
        .into_iter()
        .flat_map(|scope| scope.flows)
        .chain(doc.collaborations().flat_map(|c| &c.flows))
}
fn find_flow<'a>(doc: &'a BpmnDocument, id: &ElementSelector) -> Option<&'a Flow> {
    all_flows(doc).find(|f| f.id == *id)
}
fn expanded(doc: &BpmnDocument, id: &ElementSelector) -> bool {
    doc.diagrams
        .iter()
        .flat_map(|d| &d.shapes)
        .find(|s| s.element == *id)
        .is_some_and(|s| s.is_expanded == Some(true))
}
fn event_parts(node: &Node) -> (&[EventDefinition], bool, Option<bool>) {
    match &node.details {
        Some(NodeDetails::Event {
            definitions,
            cancel_activity,
            is_interrupting,
            ..
        }) => (
            definitions,
            *cancel_activity == Some(false),
            *is_interrupting,
        ),
        Some(NodeDetails::BoundaryEvent {
            cancel_activity, ..
        }) => (&[], !cancel_activity, None),
        _ => (&[], false, None),
    }
}
/// The element's own entry, excluded from its options (bpmn-js `isDifferentType`).
fn current(doc: &BpmnDocument, node: &Node) -> ReplaceTarget {
    let (definitions, cancel_false, is_interrupting) = event_parts(node);
    ReplaceTarget::Node {
        kind: node.kind,
        trigger: definitions.first().map(Trigger::of),
        non_interrupting: cancel_false || is_interrupting == Some(false),
        event_sub_process: rules::is_event_subprocess(node),
        collapsed: is_sub(node.kind) && !expanded(doc, &node.id.selector()),
    }
}
fn default_flow(node: &Node) -> Option<&ElementSelector> {
    match &node.details {
        Some(NodeDetails::Gateway { default_flow, .. }) => default_flow.as_ref(),
        _ => node.activity.as_ref().and_then(|a| a.default_flow.as_ref()),
    }
}
fn can_have_default(kind: NodeKind) -> bool {
    kind.is_activity()
        || matches!(
            kind,
            NodeKind::ExclusiveGateway | NodeKind::InclusiveGateway | NodeKind::ComplexGateway
        )
}

/// Replace options for `element` (a flow node, data reference, or sequence flow), in bpmn-js
/// menu order, without the element's current type. Only types the model can represent are
/// offered: typed error/escalation/compensation starts only in event subprocesses, cancel
/// events only in or on transactions; a collapsed subprocess is not offered expanded targets
/// and an expanded one is not offered collapsing (both belong to the collapse toggle).
pub fn replace_options(doc: &BpmnDocument, element: &ElementSelector) -> Vec<ReplaceTarget> {
    use NodeKind as K;
    use Trigger as T;
    if let Some(flow) = find_flow(doc, element) {
        let Some(source) = endpoint(&flow.source).and_then(|s| queries::find_node(doc, s)) else {
            return Vec::new();
        };
        if flow.kind != FlowKind::Sequence {
            return Vec::new();
        }
        let is_default = default_flow(source) == Some(element);
        let activity = source.kind.is_activity();
        let mut options = Vec::new();
        if is_default || (activity && flow.condition.is_some()) {
            options.push(ReplaceTarget::SequenceFlow);
        }
        if can_have_default(source.kind) && !is_default {
            options.push(ReplaceTarget::DefaultFlow);
        }
        if activity && flow.condition.is_none() {
            options.push(ReplaceTarget::ConditionalFlow);
        }
        return options;
    }
    let Some(located) = rules::locate(doc, element) else {
        return Vec::new();
    };
    let Some(n) = located.node else {
        return Vec::new();
    };
    let owner = located
        .scope
        .as_ref()
        .and_then(|s| queries::find_node(doc, s));
    let in_event_sub = owner.is_some_and(rules::is_event_subprocess);
    let in_sub = owner.is_some();
    let in_transaction = owner.is_some_and(|o| o.kind == K::Transaction);
    let me = current(doc, n);
    let ReplaceTarget::Node {
        trigger: current_trigger,
        collapsed: is_collapsed,
        ..
    } = me
    else {
        unreachable!()
    };
    // Same-type events in other positions keep their definition (bpmn-js TYPED_EVENT).
    let typed: Vec<ReplaceTarget> = match current_trigger {
        Some(t @ (T::Message | T::Signal)) => {
            vec![
                event(K::StartEvent, t),
                event(K::IntermediateCatchEvent, t),
                event(K::IntermediateThrowEvent, t),
                event(K::EndEvent, t),
            ]
        }
        Some(t @ (T::Timer | T::Conditional)) => {
            vec![event(K::StartEvent, t), event(K::IntermediateCatchEvent, t)]
        }
        Some(T::Error) => vec![event(K::StartEvent, T::Error), event(K::EndEvent, T::Error)],
        Some(t @ (T::Escalation | T::Compensate)) => vec![
            event(K::StartEvent, t),
            event(K::IntermediateThrowEvent, t),
            event(K::EndEvent, t),
        ],
        _ => Vec::new(),
    }
    .into_iter()
    .filter(|t| match t {
        ReplaceTarget::Node {
            kind: K::StartEvent,
            trigger,
            ..
        } => {
            !(in_sub && !in_event_sub)
                && (in_event_sub
                    || !matches!(trigger, Some(T::Error | T::Escalation | T::Compensate)))
        }
        _ => true,
    })
    .collect();
    let blank = [
        node(K::StartEvent),
        node(K::IntermediateThrowEvent),
        node(K::EndEvent),
    ];
    let mut options: Vec<ReplaceTarget> = match n.kind {
        K::StartEvent if in_event_sub => [
            T::Message,
            T::Timer,
            T::Conditional,
            T::Signal,
            T::Error,
            T::Escalation,
            T::Compensate,
        ]
        .map(|t| event(K::StartEvent, t))
        .into_iter()
        .chain(
            [
                T::Message,
                T::Timer,
                T::Conditional,
                T::Signal,
                T::Escalation,
            ]
            .map(|t| non_interrupting(K::StartEvent, t)),
        )
        .chain(typed)
        .collect(),
        K::StartEvent if in_sub => blank.into_iter().chain(typed).collect(),
        K::StartEvent => blank
            .into_iter()
            .chain(
                [T::Message, T::Timer, T::Conditional, T::Signal].map(|t| event(K::StartEvent, t)),
            )
            .chain(typed)
            .collect(),
        K::IntermediateCatchEvent | K::IntermediateThrowEvent => blank
            .into_iter()
            .chain([
                event(K::IntermediateCatchEvent, T::Message),
                event(K::IntermediateThrowEvent, T::Message),
                event(K::IntermediateCatchEvent, T::Timer),
                event(K::IntermediateThrowEvent, T::Escalation),
                event(K::IntermediateCatchEvent, T::Conditional),
                event(K::IntermediateCatchEvent, T::Link),
                event(K::IntermediateThrowEvent, T::Link),
                event(K::IntermediateThrowEvent, T::Compensate),
                event(K::IntermediateCatchEvent, T::Signal),
                event(K::IntermediateThrowEvent, T::Signal),
            ])
            .chain(typed)
            .collect(),
        K::EndEvent => blank
            .into_iter()
            .chain(
                [
                    T::Message,
                    T::Escalation,
                    T::Error,
                    T::Cancel,
                    T::Compensate,
                    T::Signal,
                    T::Terminate,
                ]
                .into_iter()
                .filter(|t| *t != T::Cancel || in_transaction)
                .map(|t| event(K::EndEvent, t)),
            )
            .chain(typed)
            .collect(),
        K::BoundaryEvent => {
            let on_transaction = rules::attached_to(n)
                .and_then(|host| queries::find_node(doc, host))
                .is_some_and(|host| host.kind == K::Transaction);
            [
                T::Message,
                T::Timer,
                T::Escalation,
                T::Conditional,
                T::Error,
                T::Cancel,
                T::Signal,
                T::Compensate,
            ]
            .into_iter()
            .filter(|t| *t != T::Cancel || on_transaction)
            .map(|t| event(K::BoundaryEvent, t))
            .chain(
                [
                    T::Message,
                    T::Timer,
                    T::Escalation,
                    T::Conditional,
                    T::Signal,
                ]
                .map(|t| non_interrupting(K::BoundaryEvent, t)),
            )
            .collect()
        }
        _ if rules::is_gateway(n.kind) => [
            K::ExclusiveGateway,
            K::ParallelGateway,
            K::InclusiveGateway,
            K::ComplexGateway,
            K::EventBasedGateway,
        ]
        .map(node)
        .into(),
        // Expanded subprocesses, transactions, and event subprocesses keep their collapse state.
        _ if is_sub(n.kind)
            && (!is_collapsed || rules::is_event_subprocess(n) || n.kind == K::Transaction) =>
        {
            let mut options = vec![
                sub(K::Transaction, false, is_collapsed),
                sub(K::SubProcess, false, is_collapsed),
                sub(K::AdHocSubProcess, false, is_collapsed),
                sub(K::SubProcess, true, is_collapsed),
            ];
            // Collapsing goes through the collapse toggle (containers::toggle_collapse).
            if !is_collapsed && !rules::is_event_subprocess(n) && n.kind != K::Transaction {
                options.push(sub(n.kind, false, true));
            }
            options
        }
        _ if n.kind.is_activity() => [
            K::Task,
            K::UserTask,
            K::ServiceTask,
            K::SendTask,
            K::ReceiveTask,
            K::ManualTask,
            K::BusinessRuleTask,
            K::ScriptTask,
            K::CallActivity,
        ]
        .map(node)
        .into_iter()
        .chain([
            sub(K::SubProcess, false, true),
            sub(K::SubProcess, false, false),
            sub(K::AdHocSubProcess, false, true),
            sub(K::AdHocSubProcess, false, false),
        ])
        .filter(|t| {
            // A collapsed subprocess expands only as itself (the collapse toggle).
            let expands = |kind: NodeKind| {
                kind == n.kind && matches!(n.details, Some(NodeDetails::SubProcess { .. }))
            };
            !is_collapsed
                || !matches!(t, ReplaceTarget::Node { kind, collapsed: false, .. } if is_sub(*kind) && !expands(*kind))
        })
        .collect(),
        K::DataObjectReference => vec![node(K::DataStoreReference)],
        K::DataStoreReference => vec![node(K::DataObjectReference)],
        _ => Vec::new(),
    };
    let mut seen = Vec::new();
    options.retain(|t| {
        *t != me && !seen.contains(t) && {
            seen.push(t.clone());
            true
        }
    });
    options
}

/// Replace `element` with `target` (one of [`replace_options`]). A node changes type in place
/// and the follow-up deletions of flows the rules no longer allow are returned; a sequence flow
/// target returns the condition/default mutations. The caller applies everything as one batch.
pub fn replace(
    doc: &mut BpmnDocument,
    element: &ElementSelector,
    target: &ReplaceTarget,
) -> Result<Vec<Mutation>, String> {
    if !replace_options(doc, element).contains(target) {
        return Err(format!(
            "This element cannot be replaced with: {}",
            target.label()
        ));
    }
    if crate::containers::collapse_toggle_target(doc, element, target).is_some() {
        return Err("Collapse and expand subprocesses with containers::toggle_collapse".into());
    }
    if matches!(target, ReplaceTarget::Node { .. }) {
        replace_node(doc, element, target)
    } else {
        replace_flow(doc, element, target)
    }
}

fn replace_flow(
    doc: &BpmnDocument,
    id: &ElementSelector,
    target: &ReplaceTarget,
) -> Result<Vec<Mutation>, String> {
    let flow = find_flow(doc, id).ok_or("flow not found")?;
    let source = endpoint(&flow.source)
        .and_then(|s| queries::find_node(doc, s))
        .ok_or("The flow has no local source")?;
    let set_default = |value: Option<ElementSelector>| {
        if source.kind.is_activity() {
            let mut activity = source.activity.clone().unwrap_or_default();
            activity.default_flow = value;
            Mutation::SetActivityProperties {
                document_id: doc.id,
                element_id: source.id.selector(),
                activity: (activity != ActivityProperties::default()).then_some(activity),
            }
        } else {
            let mut details = source.details.clone().unwrap_or(NodeDetails::Gateway {
                default_flow: None,
                direction: None,
                instantiate: None,
                event_gateway_type: None,
                activation_condition: None,
            });
            if let NodeDetails::Gateway { default_flow, .. } = &mut details {
                *default_flow = value;
            }
            Mutation::SetNodeDetails {
                document_id: doc.id,
                element_id: source.id.selector(),
                details: (!details.is_vacuous()).then_some(details),
            }
        }
    };
    let condition = |value: Option<Expression>| Mutation::SetFlowCondition {
        document_id: doc.id,
        flow_id: id.clone(),
        condition: value,
    };
    let is_default = default_flow(source) == Some(id);
    let mut batch = Vec::new();
    match target {
        ReplaceTarget::DefaultFlow => {
            batch.push(set_default(Some(id.clone())));
            if flow.condition.is_some() {
                batch.push(condition(None));
            }
        }
        ReplaceTarget::ConditionalFlow => {
            if is_default {
                batch.push(set_default(None));
            }
            batch.push(condition(Some("".into())));
        }
        _ => {
            if is_default {
                batch.push(set_default(None));
            }
            if flow.condition.is_some() {
                batch.push(condition(None));
            }
        }
    }
    Ok(batch)
}

fn refuse<T>(what: &str, fits: &str) -> Result<T, String> {
    Err(format!(
        "Its {what} only fits {fits}; clear it before changing the type"
    ))
}
fn has_cancel(node: &Node) -> bool {
    event_parts(node)
        .0
        .iter()
        .any(|d| matches!(d, EventDefinition::Cancel { .. }))
}

/// The replaced node's details and activity properties, or why the change would lose data.
fn morph(doc: &BpmnDocument, old: &Node, target: &ReplaceTarget) -> Result<Node, String> {
    use NodeKind as K;
    let ReplaceTarget::Node {
        kind,
        trigger,
        non_interrupting,
        event_sub_process,
        ..
    } = *target
    else {
        unreachable!()
    };
    let id = old.id.selector();
    let mut new = old.clone();
    new.kind = kind;
    if kind.is_activity() {
        let attached: Vec<&Node> = queries::scopes(doc)
            .into_iter()
            .flat_map(|s| s.nodes)
            .filter(|n| rules::attached_to(n) == Some(&id))
            .collect();
        if event_sub_process && !attached.is_empty() {
            return Err(
                "An event subprocess cannot have boundary events; remove them first".into(),
            );
        }
        if old.kind == K::Transaction && kind != K::Transaction {
            let inner_cancel = match &old.details {
                Some(NodeDetails::SubProcess { content }) => content
                    .nodes
                    .iter()
                    .any(|n| n.kind == K::EndEvent && has_cancel(n)),
                _ => false,
            };
            if inner_cancel || attached.iter().any(|n| has_cancel(n)) {
                return refuse("cancel events", "transactions");
            }
        }
        if let Some(a) = &mut new.activity {
            if a.implementation.is_some()
                && !matches!(
                    kind,
                    K::ServiceTask
                        | K::SendTask
                        | K::ReceiveTask
                        | K::UserTask
                        | K::BusinessRuleTask
                )
            {
                return refuse(
                    "implementation",
                    "service, send, receive, user, and business rule tasks",
                );
            }
            if a.message_ref.is_some() && !matches!(kind, K::SendTask | K::ReceiveTask) {
                return refuse("message reference", "send and receive tasks");
            }
            if a.operation_ref.is_some()
                && !matches!(kind, K::ServiceTask | K::SendTask | K::ReceiveTask)
            {
                return refuse("operation reference", "service, send, and receive tasks");
            }
            if a.instantiate.is_some() && kind != K::ReceiveTask {
                return refuse("instantiate flag", "receive tasks");
            }
            if (a.script.is_some() || a.script_format.is_some()) && kind != K::ScriptTask {
                return refuse("script", "script tasks");
            }
            if a.transaction_method.is_some() && kind != K::Transaction {
                return refuse("transaction method", "transactions");
            }
            if (a.ad_hoc_ordering.is_some()
                || a.cancel_remaining_instances.is_some()
                || a.completion_condition.is_some())
                && kind != K::AdHocSubProcess
            {
                return refuse("ad-hoc settings", "ad-hoc subprocesses");
            }
            if !a.renderings.is_empty() && kind != K::UserTask {
                return refuse("renderings", "user tasks");
            }
            a.triggered_by_event = if event_sub_process {
                Some(true)
            } else if is_sub(kind) && a.triggered_by_event == Some(false) {
                Some(false)
            } else {
                None
            };
        } else if event_sub_process {
            new.activity = Some(ActivityProperties {
                triggered_by_event: Some(true),
                ..Default::default()
            });
        }
        if new.activity.as_ref() == Some(&ActivityProperties::default()) {
            new.activity = None;
        }
        new.details = match (&old.details, kind) {
            (Some(details @ NodeDetails::CallActivity { .. }), K::CallActivity) => {
                Some(details.clone())
            }
            (_, K::CallActivity) => Some(NodeDetails::CallActivity {
                called_element: None,
            }),
            (Some(details @ NodeDetails::SubProcess { .. }), _) if is_sub(kind) => {
                Some(details.clone())
            }
            (_, _) if is_sub(kind) => Some(NodeDetails::SubProcess {
                content: Box::default(),
            }),
            _ => None,
        };
        if let Some(NodeDetails::CallActivity {
            called_element: Some(_),
        }) = &old.details
            && kind != K::CallActivity
        {
            return refuse("called element", "call activities");
        }
        if let Some(NodeDetails::SubProcess { content }) = &old.details
            && !is_sub(kind)
        {
            if **content != FlowContainer::default() {
                return Err("Its child elements would be lost; remove them first".into());
            }
            if doc.diagrams.iter().any(|d| d.plane == id) {
                return Err("It has its own diagram; delete that diagram first".into());
            }
        }
    } else if rules::is_event(kind) {
        let (
            definitions,
            attached_to,
            definition_refs,
            parallel_multiple,
            is_interrupting,
            properties,
            io,
        ) = match &old.details {
            Some(NodeDetails::Event {
                definitions,
                attached_to,
                definition_refs,
                parallel_multiple,
                is_interrupting,
                properties,
                io,
                ..
            }) => (
                definitions.clone(),
                attached_to.clone(),
                definition_refs.clone(),
                *parallel_multiple,
                *is_interrupting,
                properties.clone(),
                io.clone(),
            ),
            Some(NodeDetails::BoundaryEvent { attached_to, .. }) => (
                Vec::new(),
                Some(attached_to.clone()),
                Vec::new(),
                None,
                None,
                Vec::new(),
                None,
            ),
            _ => (Vec::new(), None, Vec::new(), None, None, Vec::new(), None),
        };
        let same = definitions.len() <= 1 && definitions.first().map(Trigger::of) == trigger;
        let definitions = if same {
            definitions
        } else {
            if !definition_refs.is_empty() || definitions.len() > 1 {
                return Err(
                    "It uses several or shared event definitions; edit them in the inspector first"
                        .into(),
                );
            }
            if definitions.iter().any(carries_data) {
                return Err(format!(
                    "Its {} definition carries data a {} cannot hold; clear it first",
                    Trigger::of(&definitions[0]).name().to_lowercase(),
                    target.label().to_lowercase()
                ));
            }
            trigger.map(Trigger::definition).into_iter().collect()
        };
        let catch = matches!(
            kind,
            K::StartEvent | K::IntermediateCatchEvent | K::BoundaryEvent
        );
        if io
            .as_ref()
            .is_some_and(|io| matches!(io, EventIo::Catch { .. }) != catch)
        {
            return Err(format!(
                "Its data {} do not fit a {}; remove them first",
                if catch { "inputs" } else { "outputs" },
                target.label().to_lowercase()
            ));
        }
        if parallel_multiple.is_some() && !catch {
            return refuse("parallel multiple flag", "catch events");
        }
        let boundary = kind == K::BoundaryEvent;
        let details = NodeDetails::Event {
            definitions,
            attached_to: attached_to.filter(|_| boundary),
            cancel_activity: boundary.then_some(!non_interrupting),
            definition_refs,
            parallel_multiple,
            is_interrupting: if kind != K::StartEvent {
                None
            } else if non_interrupting {
                Some(false)
            } else {
                is_interrupting.filter(|v| *v)
            },
            properties,
            io,
        };
        new.details = (!details.is_vacuous()).then_some(details);
    } else if rules::is_gateway(kind) {
        if let Some(NodeDetails::Gateway {
            default_flow,
            instantiate,
            event_gateway_type,
            activation_condition,
            ..
        }) = &mut new.details
        {
            if activation_condition.is_some() && kind != K::ComplexGateway {
                return refuse("activation condition", "complex gateways");
            }
            if kind != K::EventBasedGateway {
                // Explicit defaults (instantiate false, exclusive type) carry no data.
                if *instantiate == Some(true)
                    || *event_gateway_type == Some(EventGatewayType::Parallel)
                {
                    return refuse("event gateway settings", "event-based gateways");
                }
                *instantiate = None;
                *event_gateway_type = None;
            }
            // bpmn-js: a gateway type without default flows drops the default marker.
            if !can_have_default(kind) {
                *default_flow = None;
            }
        }
        if new.details.as_ref().is_some_and(NodeDetails::is_vacuous) {
            new.details = None;
        }
    } else {
        // Data object reference <-> data store reference: the reference to the backing data
        // object or data store is dropped (that root element itself stays), as in bpmn-js.
        new.details = (kind == K::DataStoreReference).then_some(NodeDetails::DataStoreReference {
            data_store_ref: None,
        });
    }
    Ok(new)
}

/// Size class: shapes keep their size when replaced within one class.
fn size_class(kind: NodeKind, expanded: bool) -> (u8, NodeKind) {
    match kind {
        _ if rules::is_event(kind) => (0, NodeKind::StartEvent),
        _ if rules::is_gateway(kind) => (1, NodeKind::ExclusiveGateway),
        _ if is_sub(kind) && expanded => (2, NodeKind::SubProcess),
        _ if kind.is_activity() => (3, NodeKind::Task),
        _ => (4, kind),
    }
}

fn replace_node(
    doc: &mut BpmnDocument,
    id: &ElementSelector,
    target: &ReplaceTarget,
) -> Result<Vec<Mutation>, String> {
    let ReplaceTarget::Node {
        kind, collapsed, ..
    } = *target
    else {
        unreachable!()
    };
    let old = queries::find_node(doc, id).ok_or("element not found")?;
    let old_class = size_class(old.kind, expanded(doc, id));
    let new = morph(doc, old, target)?;
    *crate::commands::node_mut(doc, id).map_err(|_| "element not found")? = new;

    let new_expanded = is_sub(kind) && !collapsed;
    let new_class = size_class(kind, new_expanded);
    let size = match new_class.0 {
        2 => (350.0, 200.0),
        3 => (100.0, 80.0),
        _ => modeling::default_size(kind),
    };
    for diagram in &mut doc.diagrams {
        let Some(shape) = diagram.shapes.iter_mut().find(|s| s.element == *id) else {
            continue;
        };
        shape.is_expanded = is_sub(kind).then_some(new_expanded);
        if old_class == new_class {
            continue;
        }
        let o = shape.bounds.clone();
        let (cx, cy) = (o.x + o.width / 2.0, o.y + o.height / 2.0);
        let b = Bounds {
            x: cx - size.0 / 2.0,
            y: cy - size.1 / 2.0,
            width: size.0,
            height: size.1,
        };
        if modeling::has_external_label(kind) {
            let label = shape.label.get_or_insert_with(Label::default);
            match &mut label.bounds {
                Some(l) => l.y += (b.y + b.height) - (o.y + o.height),
                None => label.bounds = Some(modeling::external_label_bounds(&b)),
            }
        }
        shape.bounds = b.clone();
        // Edge ends docked on the old outline move proportionally onto the new one.
        for edge in &mut diagram.edges {
            if let Some(points) = geometry::redock(&edge.waypoints, &o, &b) {
                edge.waypoints = points;
            }
        }
    }
    Ok(incompatible_flows(doc, id, kind))
}

/// Attach event `id` to `host` as a boundary event, or detach it (`host` none) into an
/// intermediate catch event, or a throw event when it has no definition (bpmn-js attach/detach
/// behaviors). Definitions are kept; flows the rules no longer allow are returned as deletions.
pub(crate) fn set_attachment(
    doc: &mut BpmnDocument,
    id: &ElementSelector,
    host: Option<&ElementSelector>,
) -> Result<Vec<Mutation>, String> {
    let old = queries::find_node(doc, id).ok_or("element not found")?;
    let (definitions, non_interrupting, _) = event_parts(old);
    let trigger = definitions.first().map(Trigger::of);
    let kind = match host {
        Some(_) => NodeKind::BoundaryEvent,
        None if trigger.is_none() => NodeKind::IntermediateThrowEvent,
        None => NodeKind::IntermediateCatchEvent,
    };
    let target = ReplaceTarget::Node {
        kind,
        trigger,
        non_interrupting: host.is_some() && non_interrupting,
        event_sub_process: false,
        collapsed: false,
    };
    let mut new = morph(doc, old, &target)?;
    if let Some(host) = host {
        let mut details = new.details.take().unwrap_or(NodeDetails::Event {
            definitions: Vec::new(),
            attached_to: None,
            cancel_activity: Some(true),
            definition_refs: Vec::new(),
            parallel_multiple: None,
            is_interrupting: None,
            properties: Vec::new(),
            io: None,
        });
        if let NodeDetails::Event { attached_to, .. } = &mut details {
            *attached_to = Some(ModelReference::Local(host.clone()));
        }
        new.details = Some(details);
    }
    *crate::commands::node_mut(doc, id).map_err(|_| "element not found")? = new;
    Ok(incompatible_flows(doc, id, kind))
}

/// Re-check the sequence and message flows of `id`, now of `kind` (bpmn-js
/// ReplaceConnectionBehavior): deletions for the flows the rules no longer allow.
fn incompatible_flows(doc: &BpmnDocument, id: &ElementSelector, kind: NodeKind) -> Vec<Mutation> {
    let mut doomed: Vec<ElementSelector> = Vec::new();
    for flow in all_flows(doc) {
        let (Some(source), Some(target)) = (endpoint(&flow.source), endpoint(&flow.target)) else {
            continue;
        };
        if (source != id && target != id) || flow.kind == FlowKind::Association {
            continue;
        }
        let allowed = matches!(
            (rules::can_connect(doc, source, target), flow.kind),
            (Ok(rules::ConnectionKind::Sequence), FlowKind::Sequence)
                | (Ok(rules::ConnectionKind::Message), FlowKind::Message)
        );
        if !allowed {
            doomed.push(flow.id.selector());
        } else if kind == NodeKind::EventBasedGateway && source == id {
            // bpmn-js: receive tasks after an event-based gateway lose their boundary events.
            for scope in queries::scopes(doc) {
                for n in scope.nodes {
                    if rules::attached_to(n) == Some(target)
                        && queries::find_node(doc, target)
                            .is_some_and(|t| t.kind == NodeKind::ReceiveTask)
                        && !doomed.contains(&n.id.selector())
                    {
                        doomed.push(n.id.selector());
                    }
                }
            }
        }
    }
    doomed
        .into_iter()
        .map(|element_id| Mutation::Delete {
            document_id: doc.id,
            element_id,
        })
        .collect()
}
