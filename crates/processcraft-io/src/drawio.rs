//! One BPMN diagram as an uncompressed draw.io (diagrams.net) file.
//!
//! Shapes use draw.io's own BPMN 2.0 library (`mxgraph.bpmn.task2`, `event`, `gateway2`,
//! `data2`, `conversation2`) with the style keys of its sidebar, so the result opens as
//! native, editable draw.io BPMN shapes. This is a visual export: BPMN semantics that
//! draw.io has no style for are not carried, and nothing is read back.
//!
//! ponytail: every cell is a child of the root layer at absolute DI coordinates, so
//! dragging a pool in draw.io does not drag its contents; nest cells if that matters.
use processcraft_core::{
    bpmn_collaboration::ConversationKind,
    bpmn_model::{
        AssociationDirection, EventDefinition, EventGatewayType, EventReference,
        LoopCharacteristics, NodeDetails,
    },
    bpmn_xml::{MixedXml, XmlNode},
    ids::ElementSelector,
    model::{BpmnDocument, Capability, FlowContainer, FlowKind, Node, NodeKind, Point},
};
use std::{collections::BTreeMap, fmt::Write};

#[derive(Debug)]
pub struct DrawioExport {
    pub bytes: Vec<u8>,
    /// Shapes or edges drawn as plain draw.io geometry because draw.io has no BPMN style for them.
    pub approximated: Vec<String>,
}

enum Element<'a> {
    Node(&'a Node),
    Pool(Option<&'a str>),
    Lane(Option<&'a str>),
    Group,
    DataStore(Option<&'a str>),
    Port(Option<&'a str>, bool),
    Conversation(Option<&'a str>, &'a ConversationKind),
    Flow(&'a processcraft_core::model::Flow),
    DataAssociation,
    ConversationLink(Option<&'a str>),
}

type Scope<'a> = (
    &'a [Node],
    &'a [processcraft_core::model::Lane],
    &'a [processcraft_core::model::Flow],
    &'a [processcraft_core::bpmn_model::Group],
    &'a [processcraft_core::bpmn_data::DataAssociation],
);

fn index(document: &BpmnDocument) -> BTreeMap<ElementSelector, Element<'_>> {
    let mut map = BTreeMap::new();
    let mut scopes: Vec<Scope> = Vec::new();
    for p in &document.processes {
        scopes.push((
            &p.nodes,
            &p.lanes,
            &p.flows,
            &p.groups,
            &p.data_associations,
        ));
        if let Some(io) = &p.io_specification {
            for (port, output) in io
                .inputs
                .iter()
                .map(|p| (p, false))
                .chain(io.outputs.iter().map(|p| (p, true)))
            {
                if let Some(id) = &port.id {
                    map.insert(id.selector(), Element::Port(port.name.as_deref(), output));
                }
            }
        }
    }
    // Nesting is bounded by the core model's own scope limits.
    while let Some((nodes, lanes, flows, groups, associations)) = scopes.pop() {
        for node in nodes {
            map.insert(node.id.selector(), Element::Node(node));
            if let Some(NodeDetails::SubProcess { content }) = &node.details {
                let FlowContainer {
                    nodes,
                    lanes,
                    flows,
                    groups,
                    data_associations,
                    ..
                } = content.as_ref();
                scopes.push((nodes, lanes, flows, groups, data_associations));
            }
        }
        for lane in lanes {
            map.insert(lane.id.selector(), Element::Lane(lane.name.as_deref()));
        }
        for flow in flows {
            map.insert(flow.id.selector(), Element::Flow(flow));
        }
        for group in groups {
            map.insert(group.id.selector(), Element::Group);
        }
        for association in associations {
            if let Some(id) = &association.id {
                map.insert(id.selector(), Element::DataAssociation);
            }
        }
    }
    for store in &document.data_stores {
        if let Some(id) = &store.id {
            map.insert(id.selector(), Element::DataStore(store.name.as_deref()));
        }
    }
    for collaboration in document.collaborations() {
        for participant in &collaboration.participants {
            map.insert(
                participant.id.selector(),
                Element::Pool(participant.name.as_deref()),
            );
        }
        for flow in &collaboration.flows {
            map.insert(flow.id.selector(), Element::Flow(flow));
        }
        for node in &collaboration.conversation_nodes {
            map.insert(
                node.id.selector(),
                Element::Conversation(node.name.as_deref(), &node.kind),
            );
        }
        for link in &collaboration.conversation_links {
            map.insert(
                link.id.selector(),
                Element::ConversationLink(link.name.as_deref()),
            );
        }
    }
    map
}

const TASK: &str = "shape=mxgraph.bpmn.task2;whiteSpace=wrap;rectStyle=rounded;size=10;html=1;container=1;expand=0;collapsible=0;";
const EVENT: &str = "shape=mxgraph.bpmn.event;html=1;verticalLabelPosition=bottom;labelBackgroundColor=#ffffff;verticalAlign=top;align=center;perimeter=ellipsePerimeter;outlineConnect=0;aspect=fixed;";
const GATEWAY: &str = "shape=mxgraph.bpmn.gateway2;html=1;verticalLabelPosition=bottom;labelBackgroundColor=#ffffff;verticalAlign=top;align=center;perimeter=rhombusPerimeter;outlineConnect=0;";
const DATA: &str = "shape=mxgraph.bpmn.data2;labelPosition=center;verticalLabelPosition=bottom;align=center;verticalAlign=top;size=15;html=1;";
const SEQUENCE: &str = "html=1;rounded=0;endArrow=blockThin;endFill=1;";
const DOTTED: &str = "html=1;rounded=0;dashed=1;dashPattern=1 4;endFill=0;startFill=0;";

fn symbol(definition: &EventDefinition) -> &'static str {
    match definition {
        EventDefinition::Message { .. } => "message",
        EventDefinition::Signal { .. } => "signal",
        EventDefinition::Error { .. } => "error",
        EventDefinition::Escalation { .. } => "escalation",
        EventDefinition::Conditional { .. } => "conditional",
        EventDefinition::Link { .. } => "link",
        EventDefinition::Cancel { .. } => "cancel",
        EventDefinition::Compensate { .. } => "compensation",
        EventDefinition::Terminate { .. } => "terminate",
        EventDefinition::Timer { .. } => "timer",
    }
}

fn event_style(node: &Node, document: &BpmnDocument) -> String {
    let (definitions, parallel, interrupting, cancel) = match &node.details {
        Some(NodeDetails::Event {
            definitions,
            definition_refs,
            parallel_multiple,
            is_interrupting,
            cancel_activity,
            ..
        }) => {
            let mut all: Vec<_> = definitions.iter().collect();
            all.extend(definition_refs.iter().filter_map(|r| {
                match r {
                    EventReference::Local(selector) => document
                        .root_event_definitions
                        .iter()
                        .find(|d| d.id().is_some_and(|id| &id.selector() == selector)),
                    EventReference::Qualified(_) => None,
                }
            }));
            (
                all,
                *parallel_multiple == Some(true),
                *is_interrupting,
                *cancel_activity,
            )
        }
        Some(NodeDetails::BoundaryEvent {
            cancel_activity, ..
        }) => (Vec::new(), false, None, Some(*cancel_activity)),
        _ => (Vec::new(), false, None, None),
    };
    let outline = match node.kind {
        NodeKind::StartEvent if interrupting == Some(false) => "eventNonint",
        NodeKind::StartEvent => "standard",
        NodeKind::EndEvent => "end",
        NodeKind::IntermediateCatchEvent => "catching",
        NodeKind::IntermediateThrowEvent => "throwing",
        _ if cancel == Some(false) => "boundNonint",
        _ => "boundInt",
    };
    let symbol = match definitions.as_slice() {
        [] => "general",
        [one] => symbol(one),
        _ if parallel => "parallelMultiple",
        _ => "multiple",
    };
    format!("{EVENT}outline={outline};symbol={symbol};")
}

fn activity_style(node: &Node, expanded: bool) -> String {
    let mut style = TASK.to_owned();
    let marker = match node.kind {
        NodeKind::UserTask => "user",
        NodeKind::ManualTask => "manual",
        NodeKind::ServiceTask => "service",
        NodeKind::SendTask => "send",
        NodeKind::ReceiveTask => "receive",
        NodeKind::ScriptTask => "script",
        NodeKind::BusinessRuleTask => "businessRule",
        _ => "abstract",
    };
    let _ = write!(style, "taskMarker={marker};");
    let activity = node.activity.as_ref();
    match node.kind {
        NodeKind::CallActivity => style.push_str("bpmnShapeType=call;"),
        NodeKind::Transaction => style.push_str("bpmnShapeType=transaction;"),
        NodeKind::SubProcess | NodeKind::AdHocSubProcess
            if activity.is_some_and(|a| a.triggered_by_event == Some(true)) =>
        {
            style.push_str("bpmnShapeType=subprocess;outline=none;symbol=general;")
        }
        _ => {}
    }
    let container = matches!(
        node.kind,
        NodeKind::SubProcess
            | NodeKind::AdHocSubProcess
            | NodeKind::Transaction
            | NodeKind::CallActivity
    );
    if container && expanded {
        style.push_str("verticalAlign=top;align=left;spacingLeft=5;");
    } else if container && node.kind != NodeKind::CallActivity {
        style.push_str("isLoopSub=1;");
    }
    if node.kind == NodeKind::AdHocSubProcess {
        style.push_str("isAdHoc=1;");
    }
    match &node.loop_characteristics {
        Some(LoopCharacteristics::Standard { .. }) => style.push_str("isLoopStandard=1;"),
        Some(LoopCharacteristics::MultiInstance { is_sequential, .. }) => {
            style.push_str(if *is_sequential == Some(true) {
                "isLoopMultiSeq=1;"
            } else {
                "isLoopMultiParallel=1;"
            })
        }
        None => {}
    }
    if activity.is_some_and(|a| a.is_for_compensation == Some(true)) {
        style.push_str("isLoopComp=1;");
    }
    style
}

fn node_style(node: &Node, expanded: bool, document: &BpmnDocument) -> String {
    match node.kind {
        NodeKind::StartEvent
        | NodeKind::EndEvent
        | NodeKind::IntermediateCatchEvent
        | NodeKind::IntermediateThrowEvent
        | NodeKind::BoundaryEvent => event_style(node, document),
        NodeKind::ExclusiveGateway => format!("{GATEWAY}outline=none;symbol=none;gwType=exclusive;"),
        NodeKind::ParallelGateway => format!("{GATEWAY}outline=none;symbol=none;gwType=parallel;"),
        NodeKind::ComplexGateway => format!("{GATEWAY}outline=none;symbol=none;gwType=complex;"),
        NodeKind::InclusiveGateway => format!("{GATEWAY}outline=end;symbol=general;"),
        NodeKind::EventBasedGateway => {
            let parallel = matches!(
                node.details,
                Some(NodeDetails::Gateway {
                    event_gateway_type: Some(EventGatewayType::Parallel),
                    ..
                })
            );
            let symbol = if parallel { "parallelMultiple" } else { "general" };
            format!("{GATEWAY}outline=standard;symbol={symbol};")
        }
        NodeKind::DataObjectReference => DATA.to_owned(),
        NodeKind::DataStoreReference => "shape=datastore;html=1;labelPosition=center;verticalLabelPosition=bottom;align=center;verticalAlign=top;".to_owned(),
        NodeKind::TextAnnotation => "shape=partialRectangle;html=1;top=0;right=0;bottom=0;fillColor=none;align=left;spacingLeft=5;whiteSpace=wrap;".to_owned(),
        _ => activity_style(node, expanded),
    }
}

fn node_label(node: &Node) -> String {
    let Some(NodeDetails::TextAnnotation {
        text: Some(text), ..
    }) = &node.details
    else {
        return node.name.clone().unwrap_or_default();
    };
    match text {
        MixedXml::Text(text) => text.clone(),
        // Markup is projected as inert characters, as in the renderer.
        MixedXml::Structured(content) => {
            let mut out = String::new();
            let mut pending: Vec<_> = content.nodes.iter().rev().collect();
            while let Some(part) = pending.pop() {
                match part {
                    XmlNode::Text { text } => out.push_str(text),
                    XmlNode::Element { element } => pending.extend(element.children.iter().rev()),
                    XmlNode::Comment { .. } | XmlNode::ProcessingInstruction { .. } => {}
                }
            }
            out
        }
    }
}

/// draw.io labels are HTML (`html=1`) stored in an XML attribute: escape for both.
fn label(text: &str) -> String {
    let html = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\n', "<br>");
    attribute(&html)
}

fn attribute(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn point(out: &mut String, p: &Point, role: &str) {
    let _ = write!(out, "<mxPoint x=\"{}\" y=\"{}\"{role}/>", p.x, p.y);
}

/// Export one diagram of an editable document as an uncompressed `.drawio` file.
pub fn export_drawio(
    document: &BpmnDocument,
    diagram_id: &ElementSelector,
) -> Result<DrawioExport, &'static str> {
    if !matches!(document.capability, Capability::Editable) {
        return Err("Document does not have a qualified editable model");
    }
    let diagram = document
        .diagrams
        .iter()
        .find(|d| &d.id.selector() == diagram_id)
        .ok_or("Diagram not found")?;
    let elements = index(document);
    let mut approximated = Vec::new();
    let mut cells = String::new();
    // Edge ends may name a DI shape or a semantic element; both resolve to the first cell.
    let mut cell_of: BTreeMap<ElementSelector, String> = BTreeMap::new();
    for (n, shape) in diagram.shapes.iter().enumerate() {
        let id = format!("s{n}");
        let b = &shape.bounds;
        let (style, value) = match elements.get(&shape.element) {
            Some(Element::Node(node)) => (
                node_style(node, shape.is_expanded == Some(true), document),
                node_label(node),
            ),
            Some(Element::Pool(name)) | Some(Element::Lane(name)) => {
                let horizontal = shape.is_horizontal != Some(false);
                (
                    format!(
                        "swimlane;html=1;startSize=30;collapsible=0;horizontal={};{}",
                        u8::from(!horizontal),
                        if matches!(elements.get(&shape.element), Some(Element::Lane(_))) {
                            "swimlaneLine=1;fillColor=none;"
                        } else {
                            ""
                        }
                    ),
                    name.unwrap_or_default().to_owned(),
                )
            }
            Some(Element::Group) => (
                "rounded=1;arcSize=10;dashed=1;fillColor=none;dashPattern=8 3 1 3;strokeWidth=2;whiteSpace=wrap;html=1;verticalAlign=top;".into(),
                String::new(),
            ),
            Some(Element::DataStore(name)) => (
                "shape=datastore;html=1;labelPosition=center;verticalLabelPosition=bottom;align=center;verticalAlign=top;".into(),
                name.unwrap_or_default().to_owned(),
            ),
            Some(Element::Port(name, output)) => (
                format!(
                    "{DATA}bpmnTransferType={};",
                    if *output { "output" } else { "input" }
                ),
                name.unwrap_or_default().to_owned(),
            ),
            Some(Element::Conversation(name, kind)) => (
                format!(
                    "shape=mxgraph.bpmn.conversation2;perimeter=hexagonPerimeter2;whiteSpace=wrap;html=1;bpmnConversationType={};{}",
                    if matches!(kind, ConversationKind::CallConversation) { "call" } else { "conv" },
                    if matches!(kind, ConversationKind::SubConversation) { "isLoopSub=1;" } else { "" }
                ),
                name.unwrap_or_default().to_owned(),
            ),
            _ => {
                approximated.push(format!("shape {}", shape.id));
                (
                    "rounded=1;whiteSpace=wrap;html=1;dashed=1;fillColor=none;".into(),
                    String::new(),
                )
            }
        };
        let _ = write!(
            cells,
            "<mxCell id=\"{id}\" value=\"{}\" style=\"{}\" vertex=\"1\" parent=\"1\"><mxGeometry x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" as=\"geometry\"/></mxCell>",
            label(&value),
            attribute(&style),
            b.x,
            b.y,
            b.width,
            b.height
        );
        cell_of.insert(shape.id.selector(), id.clone());
        cell_of.entry(shape.element.clone()).or_insert(id);
    }
    let defaults: Vec<&ElementSelector> = elements
        .values()
        .filter_map(|e| match e {
            Element::Node(node) => match &node.details {
                Some(NodeDetails::Gateway {
                    default_flow: Some(flow),
                    ..
                }) => Some(flow),
                _ => node.activity.as_ref()?.default_flow.as_ref(),
            },
            _ => None,
        })
        .collect();
    let reference_cell = |reference: &Option<EventReference>| match reference {
        Some(EventReference::Local(selector)) => cell_of.get(selector),
        _ => None,
    };
    for (n, edge) in diagram.edges.iter().enumerate() {
        let element = elements.get(&edge.element);
        let (style, value) = match element {
            Some(Element::Flow(flow)) => match flow.kind {
                FlowKind::Sequence => {
                    let mut style = SEQUENCE.to_owned();
                    let from_activity = matches!(&flow.source, Some(EventReference::Local(s))
                        if matches!(elements.get(s), Some(Element::Node(n)) if n.kind.is_activity()));
                    if defaults.contains(&&edge.element) {
                        style.push_str("startArrow=dash;startFill=0;startSize=6;");
                    } else if flow.condition.is_some() && from_activity {
                        style.push_str("startArrow=diamondThin;startFill=0;startSize=10;");
                    }
                    (style, flow.name.clone())
                }
                FlowKind::Message => (
                    "html=1;rounded=0;dashed=1;dashPattern=8 4;endArrow=blockThin;endFill=0;startArrow=oval;startFill=0;endSize=6;startSize=4;".to_owned(),
                    flow.name.clone(),
                ),
                FlowKind::Association => {
                    let arrows = match flow.association_direction {
                        Some(AssociationDirection::One) => "endArrow=openThin;startArrow=none;",
                        Some(AssociationDirection::Both) => "endArrow=openThin;startArrow=openThin;",
                        _ => "endArrow=none;startArrow=none;",
                    };
                    (format!("{DOTTED}{arrows}"), flow.name.clone())
                }
            },
            Some(Element::DataAssociation) => {
                (format!("{DOTTED}endArrow=openThin;startArrow=none;"), None)
            }
            Some(Element::ConversationLink(name)) => (
                "html=1;rounded=0;shape=link;endArrow=none;".to_owned(),
                name.map(str::to_owned),
            ),
            _ => {
                approximated.push(format!("edge {}", edge.id));
                ("html=1;rounded=0;endArrow=none;".to_owned(), None)
            }
        };
        let (source_ref, target_ref) = match element {
            Some(Element::Flow(flow)) => {
                (reference_cell(&flow.source), reference_cell(&flow.target))
            }
            _ => (None, None),
        };
        let source = edge
            .source_element
            .as_ref()
            .and_then(|s| cell_of.get(s))
            .or(source_ref);
        let target = edge
            .target_element
            .as_ref()
            .and_then(|s| cell_of.get(s))
            .or(target_ref);
        let _ = write!(
            cells,
            "<mxCell id=\"e{n}\" value=\"{}\" style=\"{}\" edge=\"1\" parent=\"1\"",
            label(value.as_deref().unwrap_or("")),
            attribute(&style)
        );
        if let Some(source) = source {
            let _ = write!(cells, " source=\"{source}\"");
        }
        if let Some(target) = target {
            let _ = write!(cells, " target=\"{target}\"");
        }
        cells.push_str("><mxGeometry relative=\"1\" as=\"geometry\">");
        if let [first, middle @ .., last] = edge.waypoints.as_slice() {
            point(&mut cells, first, " as=\"sourcePoint\"");
            point(&mut cells, last, " as=\"targetPoint\"");
            if !middle.is_empty() {
                cells.push_str("<Array as=\"points\">");
                for p in middle {
                    point(&mut cells, p, "");
                }
                cells.push_str("</Array>");
            }
        }
        cells.push_str("</mxGeometry></mxCell>");
    }
    let name = diagram
        .name
        .as_deref()
        .filter(|n| !n.is_empty())
        .unwrap_or(&document.name);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<mxfile host=\"ProcessCraft\"><diagram id=\"processcraft\" name=\"{}\"><mxGraphModel grid=\"1\" gridSize=\"10\" guides=\"1\" connect=\"1\" arrows=\"1\" page=\"0\"><root><mxCell id=\"0\"/><mxCell id=\"1\" parent=\"0\"/>{cells}</root></mxGraphModel></diagram></mxfile>\n",
        attribute(name)
    );
    Ok(DrawioExport {
        bytes: xml.into_bytes(),
        approximated,
    })
}
