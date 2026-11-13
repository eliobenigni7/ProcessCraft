//! Modeling conventions (M5.2): a closed set of rules a project can switch on.
//!
//! A [`ConventionsProfile`] is project content applied through
//! `Mutation::SetConventionsProfile`; evaluation is a pure function of an
//! immutable project, so GUI, CLI and MCP return the same diagnostics. A rule
//! that is absent from the profile is off. The verb + object rule is a word-list
//! heuristic over English labels, not language analysis, and its message says so.
use crate::attributes::{AttributeTarget, node_target};
use crate::bpmn_model::{EventReference, NodeDetails};
use crate::commands::CommandError;
use crate::ids::{DocumentId, ElementSelector};
use crate::model::{
    BpmnDocument, Capability, DictionaryKind, DictionaryTarget, NodeKind, ProjectData,
};
use crate::validation::{Diagnostic, DiagnosticSeverity};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const MAX_VERBS: usize = 2_000;
pub const MAX_VERB_BYTES: usize = 64;
pub const MAX_ELEMENT_LIMIT: u32 = 10_000;
/// Convention output cap; one more entry says that results were omitted.
pub const MAX_DIAGNOSTICS: usize = 10_000;

/// Built-in English verb list of the verb + object rule (lower case).
pub const DEFAULT_VERBS: &[&str] = &[
    "accept",
    "access",
    "add",
    "adjust",
    "analyse",
    "analyze",
    "announce",
    "apply",
    "approve",
    "archive",
    "arrange",
    "ask",
    "assemble",
    "assess",
    "assign",
    "attach",
    "audit",
    "authorise",
    "authorize",
    "bill",
    "book",
    "build",
    "calculate",
    "call",
    "cancel",
    "capture",
    "categorise",
    "categorize",
    "change",
    "charge",
    "check",
    "choose",
    "claim",
    "classify",
    "clean",
    "clear",
    "close",
    "collect",
    "communicate",
    "compare",
    "compile",
    "complete",
    "compose",
    "compute",
    "configure",
    "confirm",
    "consolidate",
    "contact",
    "continue",
    "control",
    "convert",
    "coordinate",
    "copy",
    "correct",
    "count",
    "create",
    "debit",
    "decide",
    "define",
    "delegate",
    "delete",
    "deliver",
    "demand",
    "describe",
    "design",
    "determine",
    "develop",
    "diagnose",
    "dismiss",
    "dispatch",
    "distribute",
    "document",
    "draft",
    "edit",
    "enrol",
    "enroll",
    "ensure",
    "enter",
    "escalate",
    "estimate",
    "evaluate",
    "examine",
    "execute",
    "export",
    "extract",
    "fetch",
    "file",
    "fill",
    "finalise",
    "finalize",
    "find",
    "fix",
    "forward",
    "generate",
    "get",
    "give",
    "grant",
    "handle",
    "hire",
    "hold",
    "identify",
    "implement",
    "import",
    "inform",
    "initiate",
    "inquire",
    "inspect",
    "install",
    "invite",
    "invoice",
    "issue",
    "join",
    "launch",
    "list",
    "load",
    "lock",
    "log",
    "maintain",
    "make",
    "manage",
    "mark",
    "match",
    "measure",
    "merge",
    "monitor",
    "move",
    "name",
    "negotiate",
    "notify",
    "obtain",
    "offer",
    "open",
    "order",
    "organise",
    "organize",
    "pack",
    "pay",
    "perform",
    "pick",
    "place",
    "plan",
    "post",
    "prepare",
    "price",
    "print",
    "prioritise",
    "prioritize",
    "process",
    "procure",
    "produce",
    "provide",
    "publish",
    "purchase",
    "put",
    "qualify",
    "quote",
    "rank",
    "rate",
    "receive",
    "reconcile",
    "record",
    "refund",
    "refuse",
    "register",
    "reject",
    "release",
    "remove",
    "renew",
    "rent",
    "reorder",
    "repair",
    "replace",
    "report",
    "request",
    "reserve",
    "resolve",
    "respond",
    "restore",
    "retrieve",
    "return",
    "reuse",
    "review",
    "revise",
    "route",
    "run",
    "sample",
    "save",
    "schedule",
    "score",
    "screen",
    "search",
    "select",
    "sell",
    "send",
    "set",
    "settle",
    "share",
    "ship",
    "show",
    "sign",
    "solve",
    "sort",
    "specify",
    "split",
    "start",
    "stop",
    "store",
    "submit",
    "supply",
    "take",
    "tell",
    "test",
    "track",
    "train",
    "transfer",
    "translate",
    "trigger",
    "unlock",
    "update",
    "upload",
    "use",
    "validate",
    "verify",
    "wait",
    "withdraw",
    "wrap",
    "write",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConventionsProfile {
    /// At most one entry per rule; a rule that is absent is off.
    pub rules: Vec<ConventionRule>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ConventionRule {
    LabelRequired {
        severity: DiagnosticSeverity,
        kinds: Vec<AttributeTarget>,
    },
    ActivityVerbObject {
        severity: DiagnosticSeverity,
        /// Empty means the built-in English list.
        verbs: Vec<String>,
    },
    GatewayFlowsLabelled {
        severity: DiagnosticSeverity,
    },
    NoImplicitSplitJoin {
        severity: DiagnosticSeverity,
    },
    SingleStartEnd {
        severity: DiagnosticSeverity,
    },
    MaxElements {
        severity: DiagnosticSeverity,
        limit: u32,
    },
    RequiredAttributes {
        severity: DiagnosticSeverity,
    },
    DictionaryTerms {
        severity: DiagnosticSeverity,
    },
}

impl ConventionRule {
    pub fn severity(&self) -> DiagnosticSeverity {
        match self {
            Self::LabelRequired { severity, .. }
            | Self::ActivityVerbObject { severity, .. }
            | Self::GatewayFlowsLabelled { severity }
            | Self::NoImplicitSplitJoin { severity }
            | Self::SingleStartEnd { severity }
            | Self::MaxElements { severity, .. }
            | Self::RequiredAttributes { severity }
            | Self::DictionaryTerms { severity } => *severity,
        }
    }
    /// The serde tag; also the "one entry per rule" key.
    pub fn name(&self) -> &'static str {
        match self {
            Self::LabelRequired { .. } => "labelRequired",
            Self::ActivityVerbObject { .. } => "activityVerbObject",
            Self::GatewayFlowsLabelled { .. } => "gatewayFlowsLabelled",
            Self::NoImplicitSplitJoin { .. } => "noImplicitSplitJoin",
            Self::SingleStartEnd { .. } => "singleStartEnd",
            Self::MaxElements { .. } => "maxElements",
            Self::RequiredAttributes { .. } => "requiredAttributes",
            Self::DictionaryTerms { .. } => "dictionaryTerms",
        }
    }
}

impl ConventionsProfile {
    /// All eight rules as warnings; at most 50 shapes per diagram.
    pub fn recommended() -> Self {
        let severity = DiagnosticSeverity::Warning;
        Self {
            rules: vec![
                ConventionRule::LabelRequired {
                    severity,
                    kinds: vec![
                        AttributeTarget::Activity,
                        AttributeTarget::Event,
                        AttributeTarget::Lane,
                        AttributeTarget::Participant,
                    ],
                },
                ConventionRule::ActivityVerbObject {
                    severity,
                    verbs: vec![],
                },
                ConventionRule::GatewayFlowsLabelled { severity },
                ConventionRule::NoImplicitSplitJoin { severity },
                ConventionRule::SingleStartEnd { severity },
                ConventionRule::MaxElements {
                    severity,
                    limit: 50,
                },
                ConventionRule::RequiredAttributes { severity },
                ConventionRule::DictionaryTerms { severity },
            ],
        }
    }

    pub fn validate(&self) -> Result<(), CommandError> {
        let invalid = |message: &str| Err(CommandError::InvalidParams(message.into()));
        let mut seen = HashSet::new();
        for rule in &self.rules {
            if !seen.insert(rule.name()) {
                return invalid("conventions profile has more than one entry for a rule");
            }
            match rule {
                ConventionRule::LabelRequired { kinds, .. } => {
                    if kinds.is_empty() {
                        return invalid("labelRequired needs at least one element kind");
                    }
                    if kinds.iter().collect::<HashSet<_>>().len() != kinds.len() {
                        return invalid("labelRequired lists an element kind twice");
                    }
                }
                ConventionRule::ActivityVerbObject { verbs, .. } => {
                    if verbs.len() > MAX_VERBS {
                        return invalid("verb list exceeds 2000 entries");
                    }
                    for verb in verbs {
                        if verb.is_empty()
                            || verb.len() > MAX_VERB_BYTES
                            || verb.chars().any(char::is_whitespace)
                        {
                            return invalid("a verb must be one word of 1..64 bytes");
                        }
                    }
                }
                ConventionRule::MaxElements { limit, .. }
                    if !(1..=MAX_ELEMENT_LIMIT).contains(limit) =>
                {
                    return invalid("maxElements limit must be 1..10000");
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// `ProjectData::validate` hook: a stored profile must satisfy the same bounds
/// as the mutation payload.
pub fn validate_project(project: &ProjectData) -> Result<(), String> {
    match &project.conventions {
        Some(profile) => profile.validate().map_err(|error| match error {
            CommandError::InvalidParams(message) => message,
            other => format!("{other:?}"),
        }),
        None => Ok(()),
    }
}

/// Built-in diagnostics replaced by a convention rule while that rule is on:
/// `(built-in code, rule name)`. A built-in is dropped only for documents the
/// rule evaluates (editable ones). `labelRequired` supersedes
/// `unnamedExclusiveGateway` only when it covers gateways. The other built-ins
/// (`unconnectedNode`, `gatewayDoesNotSplitOrMerge`) describe conditions no
/// rule covers.
pub const SUPERSEDED_BUILTINS: &[(&str, &str)] = &[
    ("missingStart", "singleStartEnd"),
    ("missingEnd", "singleStartEnd"),
    ("unnamedExclusiveGateway", "labelRequired"),
];

/// Removes built-in diagnostics that a rule of `profile` supersedes.
pub fn drop_superseded(
    project: &ProjectData,
    profile: &ConventionsProfile,
    builtin: &mut Vec<Diagnostic>,
) {
    let editable: HashSet<DocumentId> = project
        .documents
        .iter()
        .filter(|d| matches!(d.capability, Capability::Editable))
        .map(|d| d.id)
        .collect();
    let active = |rule_name: &str| {
        profile.rules.iter().any(|rule| {
            rule.name() == rule_name
                && match rule {
                    ConventionRule::LabelRequired { kinds, .. } => {
                        kinds.contains(&AttributeTarget::Gateway)
                    }
                    _ => true,
                }
        })
    };
    builtin.retain(|d| {
        !(editable.contains(&d.document_id)
            && SUPERSEDED_BUILTINS
                .iter()
                .any(|(code, rule)| *code == d.code && active(rule)))
    });
}

struct Item<'a> {
    selector: ElementSelector,
    target: AttributeTarget,
    name: Option<&'a str>,
}

struct Out {
    items: Vec<Diagnostic>,
    truncated: bool,
}
impl Out {
    fn push(
        &mut self,
        document_id: DocumentId,
        element: ElementSelector,
        severity: DiagnosticSeverity,
        code: &'static str,
        message: String,
    ) {
        if self.items.len() >= MAX_DIAGNOSTICS {
            self.truncated = true;
            return;
        }
        self.items.push(Diagnostic {
            document_id,
            element_id: Some(element),
            severity,
            code,
            message,
        });
    }
}

fn blank(name: Option<&str>) -> bool {
    name.is_none_or(|name| name.trim().is_empty())
}

/// Every attribute-capable element of a document with its label.
fn items<'a>(document: &'a BpmnDocument, scopes: &[crate::queries::ScopeRef<'a>]) -> Vec<Item<'a>> {
    let mut items = Vec::new();
    let process_ids: HashSet<ElementSelector> =
        document.processes.iter().map(|p| p.id.selector()).collect();
    for scope in scopes {
        if process_ids.contains(&scope.id.selector()) {
            items.push(Item {
                selector: scope.id.selector(),
                target: AttributeTarget::Process,
                name: Some(scope.name),
            });
        }
        for lane in scope.lanes {
            items.push(Item {
                selector: lane.id.selector(),
                target: AttributeTarget::Lane,
                name: lane.name.as_deref(),
            });
        }
        for node in scope.nodes {
            if let Some(target) = node_target(node.kind) {
                items.push(Item {
                    selector: node.id.selector(),
                    target,
                    name: node.name.as_deref(),
                });
            }
        }
        for flow in scope
            .flows
            .iter()
            .filter(|f| f.kind == crate::model::FlowKind::Sequence)
        {
            items.push(Item {
                selector: flow.id.selector(),
                target: AttributeTarget::SequenceFlow,
                name: flow.name.as_deref(),
            });
        }
        for data_object in scope.data_objects {
            if let Some(id) = &data_object.id {
                items.push(Item {
                    selector: id.selector(),
                    target: AttributeTarget::DataObject,
                    name: data_object.name.as_deref(),
                });
            }
        }
    }
    for collaboration in document.collaborations() {
        for participant in &collaboration.participants {
            items.push(Item {
                selector: participant.id.selector(),
                target: AttributeTarget::Participant,
                name: participant.name.as_deref(),
            });
        }
    }
    items
}

/// Convention diagnostics of every editable document, in document and rule
/// order, capped at [`MAX_DIAGNOSTICS`] plus one "omitted" entry.
pub fn diagnostics(project: &ProjectData, profile: &ConventionsProfile) -> Vec<Diagnostic> {
    let mut out = Out {
        items: Vec::new(),
        truncated: false,
    };
    for document in project
        .documents
        .iter()
        .filter(|d| matches!(d.capability, Capability::Editable))
    {
        evaluate_document(project, profile, document, &mut out);
    }
    if out.truncated
        && let Some(first) = project.documents.first()
    {
        out.items.push(Diagnostic {
            document_id: first.id,
            element_id: None,
            severity: DiagnosticSeverity::Warning,
            code: "conventionOutputTruncated",
            message: format!(
                "More than {MAX_DIAGNOSTICS} convention results; more results omitted."
            ),
        });
    }
    out.items
}

fn evaluate_document(
    project: &ProjectData,
    profile: &ConventionsProfile,
    document: &BpmnDocument,
    out: &mut Out,
) {
    let doc_id = document.id;
    let scopes = crate::queries::scopes(document);
    let items = items(document, &scopes);
    let ad_hoc: HashSet<ElementSelector> = scopes
        .iter()
        .flat_map(|scope| scope.nodes)
        .filter(|node| node.kind == NodeKind::AdHocSubProcess)
        .map(|node| node.id.selector())
        .collect();
    for rule in &profile.rules {
        let severity = rule.severity();
        match rule {
            ConventionRule::LabelRequired { kinds, .. } => {
                for item in items
                    .iter()
                    .filter(|i| kinds.contains(&i.target) && blank(i.name))
                {
                    out.push(
                        doc_id,
                        item.selector.clone(),
                        severity,
                        "conventionLabelRequired",
                        "Element has no label (modeling convention).".into(),
                    );
                }
            }
            ConventionRule::ActivityVerbObject { verbs, .. } => {
                let list: HashSet<String> = if verbs.is_empty() {
                    DEFAULT_VERBS.iter().map(|v| (*v).to_owned()).collect()
                } else {
                    verbs.iter().map(|v| v.to_lowercase()).collect()
                };
                // An empty label is left to `labelRequired`.
                for item in items
                    .iter()
                    .filter(|i| i.target == AttributeTarget::Activity && !blank(i.name))
                {
                    let words: Vec<&str> = item.name.unwrap_or("").split_whitespace().collect();
                    if words.len() < 2 || !list.contains(&words[0].to_lowercase()) {
                        out.push(
                            doc_id,
                            item.selector.clone(),
                            severity,
                            "conventionActivityVerbObject",
                            "Activity label should be a verb followed by an object. This is a word-list heuristic; the first word is checked against the verb list.".into(),
                        );
                    }
                }
            }
            ConventionRule::MaxElements { limit, .. } => {
                for diagram in &document.diagrams {
                    if diagram.shapes.len() > *limit as usize {
                        out.push(
                            doc_id,
                            diagram.plane.clone(),
                            severity,
                            "conventionMaxElements",
                            format!(
                                "Diagram has {} shapes; the convention allows {limit}.",
                                diagram.shapes.len()
                            ),
                        );
                    }
                }
            }
            ConventionRule::RequiredAttributes { .. } => {
                let have: HashSet<(&ElementSelector, crate::ids::AttributeDefinitionId)> = project
                    .attribute_values
                    .iter()
                    .filter(|v| v.source.document_id == doc_id)
                    .map(|v| (&v.source.xml_id, v.definition))
                    .collect();
                for definition in project.attribute_definitions.iter().filter(|d| d.required) {
                    for item in items
                        .iter()
                        .filter(|i| definition.applies_to.contains(&i.target))
                        .filter(|i| !have.contains(&(&i.selector, definition.id)))
                    {
                        out.push(
                            doc_id,
                            item.selector.clone(),
                            severity,
                            "conventionRequiredAttribute",
                            format!("Required attribute \"{}\" has no value.", definition.name),
                        );
                    }
                }
            }
            ConventionRule::DictionaryTerms { .. } => {
                let role_lanes: HashSet<&ElementSelector> = project
                    .dictionary_links
                    .iter()
                    .filter(|l| {
                        l.source.document_id == doc_id
                            && l.kind == DictionaryKind::Role
                            && matches!(l.target, DictionaryTarget::Entry(_))
                    })
                    .map(|l| &l.source.xml_id)
                    .collect();
                for item in items.iter().filter(|i| {
                    i.target == AttributeTarget::Lane && !role_lanes.contains(&i.selector)
                }) {
                    out.push(
                        doc_id,
                        item.selector.clone(),
                        severity,
                        "conventionDictionaryTerm",
                        "Lane is not linked to a Role entry of the dictionary.".into(),
                    );
                }
                let literals = project
                    .dictionary_links
                    .iter()
                    .filter(|l| l.source.document_id == doc_id)
                    .filter(|l| matches!(l.target, DictionaryTarget::Literal(_)))
                    .map(|l| &l.source.xml_id)
                    .chain(
                        project
                            .attribute_values
                            .iter()
                            .filter(|v| v.source.document_id == doc_id)
                            .filter(|v| {
                                matches!(&v.value, crate::attributes::AttributeValue::DictionaryRefs(refs)
                                    if refs.iter().any(|r| matches!(r, DictionaryTarget::Literal(_))))
                            })
                            .map(|v| &v.source.xml_id),
                    );
                for source in literals {
                    out.push(
                        doc_id,
                        source.clone(),
                        severity,
                        "conventionDictionaryTerm",
                        "A dictionary reference is free text instead of a dictionary entry.".into(),
                    );
                }
            }
            ConventionRule::GatewayFlowsLabelled { .. }
            | ConventionRule::NoImplicitSplitJoin { .. }
            | ConventionRule::SingleStartEnd { .. } => {
                for scope in &scopes {
                    scope_rule(rule, severity, doc_id, scope, &ad_hoc, out);
                }
            }
        }
    }
}

/// Rules that look at the sequence flows of one scope.
fn scope_rule(
    rule: &ConventionRule,
    severity: DiagnosticSeverity,
    doc_id: DocumentId,
    scope: &crate::queries::ScopeRef<'_>,
    ad_hoc: &HashSet<ElementSelector>,
    out: &mut Out,
) {
    let mut incoming: HashMap<&ElementSelector, usize> = HashMap::new();
    let mut outgoing: HashMap<&ElementSelector, Vec<&crate::model::Flow>> = HashMap::new();
    for flow in scope
        .flows
        .iter()
        .filter(|f| f.kind == crate::model::FlowKind::Sequence)
    {
        if let Some(EventReference::Local(target)) = &flow.target {
            *incoming.entry(target).or_default() += 1;
        }
        if let Some(EventReference::Local(source)) = &flow.source {
            outgoing.entry(source).or_default().push(flow);
        }
    }
    match rule {
        ConventionRule::SingleStartEnd { .. } => {
            if ad_hoc.contains(&scope.id.selector()) {
                return;
            }
            let count = |kind| scope.nodes.iter().filter(|n| n.kind == kind).count();
            if count(NodeKind::StartEvent) != 1 {
                out.push(
                    doc_id,
                    scope.id.selector(),
                    severity,
                    "conventionStartCount",
                    format!(
                        "Scope has {} start events; the convention requires exactly one.",
                        count(NodeKind::StartEvent)
                    ),
                );
            }
            if count(NodeKind::EndEvent) != 1 {
                out.push(
                    doc_id,
                    scope.id.selector(),
                    severity,
                    "conventionEndCount",
                    format!(
                        "Scope has {} end events; the convention requires exactly one.",
                        count(NodeKind::EndEvent)
                    ),
                );
            }
        }
        ConventionRule::GatewayFlowsLabelled { .. } => {
            for node in scope.nodes.iter().filter(|n| {
                matches!(
                    n.kind,
                    NodeKind::ExclusiveGateway | NodeKind::InclusiveGateway
                )
            }) {
                let flows = outgoing
                    .get(&node.id.selector())
                    .map_or(&[][..], Vec::as_slice);
                let default = match &node.details {
                    Some(NodeDetails::Gateway { default_flow, .. }) => default_flow.as_ref(),
                    _ => None,
                };
                if flows.len() < 2 {
                    continue;
                }
                for flow in flows
                    .iter()
                    .filter(|f| Some(&f.id.selector()) != default && blank(f.name.as_deref()))
                {
                    out.push(
                        doc_id,
                        flow.id.selector(),
                        severity,
                        "conventionGatewayFlowLabel",
                        "Outgoing flow of a splitting gateway has no label.".into(),
                    );
                }
            }
        }
        ConventionRule::NoImplicitSplitJoin { .. } => {
            for node in scope.nodes.iter().filter(|n| {
                matches!(
                    node_target(n.kind),
                    Some(AttributeTarget::Activity | AttributeTarget::Event)
                )
            }) {
                let selector = node.id.selector();
                if outgoing.get(&selector).map_or(0, Vec::len) > 1 {
                    out.push(
                        doc_id,
                        selector.clone(),
                        severity,
                        "conventionImplicitSplit",
                        "Element has several outgoing flows; use a gateway to split.".into(),
                    );
                }
                if incoming.get(&selector).copied().unwrap_or(0) > 1 {
                    out.push(
                        doc_id,
                        selector,
                        severity,
                        "conventionImplicitJoin",
                        "Element has several incoming flows; use a gateway to join.".into(),
                    );
                }
            }
        }
        _ => {}
    }
}
