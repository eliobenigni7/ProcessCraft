//! Inert structured-content drafts. No XML is executed or fetched.
use super::catalog::optional_text;
use processcraft_core::bpmn_xml::{
    MixedXml, NamespaceBinding, XmlAttribute, XmlContent, XmlElement, XmlNode,
};

#[derive(Clone)]
pub(super) enum MixedDraft {
    Text(String),
    Structured(ContentDraft),
}
impl MixedDraft {
    pub(super) fn new(value: &MixedXml) -> Self {
        match value {
            MixedXml::Text(text) => Self::Text(text.clone()),
            MixedXml::Structured(content) => Self::Structured(ContentDraft::new(content)),
        }
    }
    pub(super) fn build(&self) -> Result<MixedXml, String> {
        match self {
            Self::Text(text) => Ok(MixedXml::Text(text.clone())),
            Self::Structured(content) => Ok(MixedXml::Structured(content.build()?)),
        }
    }
    pub(super) fn ui(&mut self, ui: &mut egui::Ui) {
        match self {
            Self::Text(text) => {
                let label = ui.label("Text");
                ui.text_edit_multiline(text).labelled_by(label.id);
                if ui.button("Add structured content…").clicked() {
                    *self = Self::Structured(ContentDraft {
                        namespaces: Vec::new(),
                        nodes: vec![NodeDraft::Text(text.clone())],
                    });
                }
            }
            Self::Structured(content) => {
                ui.label("Structured content is stored as data.");
                content.ui(ui, 0);
            }
        }
    }
}
#[derive(Clone)]
pub(super) struct ContentDraft {
    namespaces: Vec<NamespaceBinding>,
    nodes: Vec<NodeDraft>,
}
#[derive(Clone)]
enum NodeDraft {
    Text(String),
    Element(ElementDraft),
    Comment(String),
    Instruction {
        target: String,
        data: Option<String>,
    },
}
#[derive(Clone)]
pub(super) struct AttributeDraft {
    pub(super) namespace: String,
    pub(super) local: String,
    pub(super) prefix: Option<String>,
    pub(super) value: String,
}
#[derive(Clone)]
struct ElementDraft {
    namespace: String,
    local: String,
    prefix: Option<String>,
    namespaces: Vec<NamespaceBinding>,
    attributes: Vec<AttributeDraft>,
    children: Vec<NodeDraft>,
}
pub(super) fn xml_name(value: &str) -> Result<processcraft_core::ids::XmlId, String> {
    value.parse().map_err(|_| "Enter a valid XML name.".into())
}
fn optional_name(value: &Option<String>) -> Result<Option<processcraft_core::ids::XmlId>, String> {
    value.as_ref().map(|value| xml_name(value)).transpose()
}
impl NodeDraft {
    fn new(value: &XmlNode) -> Self {
        match value {
            XmlNode::Text { text } => Self::Text(text.clone()),
            XmlNode::Comment { text } => Self::Comment(text.clone()),
            XmlNode::ProcessingInstruction { target, data } => Self::Instruction {
                target: target.to_string(),
                data: data.clone(),
            },
            XmlNode::Element { element } => Self::Element(ElementDraft {
                namespace: element.namespace.clone(),
                local: element.local.to_string(),
                prefix: element.prefix.as_ref().map(ToString::to_string),
                namespaces: element.namespaces.clone(),
                attributes: element
                    .attributes
                    .iter()
                    .map(|a| AttributeDraft {
                        namespace: a.namespace.clone(),
                        local: a.local.to_string(),
                        prefix: a.prefix.as_ref().map(ToString::to_string),
                        value: a.value.clone(),
                    })
                    .collect(),
                children: element.children.iter().map(Self::new).collect(),
            }),
        }
    }
    fn build(&self) -> Result<XmlNode, String> {
        Ok(match self {
            Self::Text(text) => XmlNode::Text { text: text.clone() },
            Self::Comment(text) => XmlNode::Comment { text: text.clone() },
            Self::Instruction { target, data } => XmlNode::ProcessingInstruction {
                target: target.clone(),
                data: data.clone(),
            },
            Self::Element(element) => XmlNode::Element {
                element: XmlElement {
                    namespace: element.namespace.clone(),
                    local: xml_name(&element.local)?,
                    prefix: optional_name(&element.prefix)?,
                    namespaces: element.namespaces.clone(),
                    attributes: element
                        .attributes
                        .iter()
                        .map(|a| {
                            Ok(XmlAttribute {
                                namespace: a.namespace.clone(),
                                local: xml_name(&a.local)?,
                                prefix: optional_name(&a.prefix)?,
                                value: a.value.clone(),
                            })
                        })
                        .collect::<Result<_, String>>()?,
                    children: element
                        .children
                        .iter()
                        .map(Self::build)
                        .collect::<Result<_, _>>()?,
                },
            },
        })
    }
}
impl ContentDraft {
    pub(super) fn new(value: &XmlContent) -> Self {
        Self {
            namespaces: value.namespaces.clone(),
            nodes: value.nodes.iter().map(NodeDraft::new).collect(),
        }
    }
    pub(super) fn build(&self) -> Result<XmlContent, String> {
        Ok(XmlContent {
            namespaces: self.namespaces.clone(),
            nodes: self
                .nodes
                .iter()
                .map(NodeDraft::build)
                .collect::<Result<_, _>>()?,
        })
    }
    pub(super) fn ui(&mut self, ui: &mut egui::Ui, depth: usize) {
        bindings_ui(ui, &mut self.namespaces);
        nodes_ui(ui, &mut self.nodes, depth);
    }
}
fn text(ui: &mut egui::Ui, label: &str, value: &mut String) {
    let label = ui.label(label);
    ui.text_edit_singleline(value).labelled_by(label.id);
}
pub(super) fn bindings_ui(ui: &mut egui::Ui, bindings: &mut Vec<NamespaceBinding>) {
    ui.collapsing("Namespace bindings", |ui| {
        let mut remove = None;
        for (i, binding) in bindings.iter_mut().enumerate() {
            ui.push_id(i, |ui| {
                text(ui, "Prefix (empty for default)", &mut binding.prefix);
                text(ui, "Namespace URI", &mut binding.uri);
                if ui.button("Remove binding").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            bindings.remove(i);
        }
        if ui
            .add_enabled(bindings.len() < 256, egui::Button::new("Add binding"))
            .clicked()
        {
            bindings.push(NamespaceBinding {
                prefix: String::new(),
                uri: String::new(),
            });
        }
    });
}
fn nodes_ui(ui: &mut egui::Ui, nodes: &mut Vec<NodeDraft>, depth: usize) {
    let mut remove = None;
    let mut move_item = None;
    for (i, node) in nodes.iter_mut().enumerate() {
        ui.push_id(i, |ui| {
            ui.separator();
            ui.label(format!("Content {}", i + 1));
            match node {
                NodeDraft::Text(value) => {
                    let label = ui.label("Text");
                    ui.text_edit_multiline(value).labelled_by(label.id);
                }
                NodeDraft::Comment(value) => {
                    text(ui, "Comment", value);
                }
                NodeDraft::Instruction { target, data } => {
                    text(ui, "Processing instruction target", target);
                    optional_text(ui, "Data", data);
                }
                NodeDraft::Element(element) => {
                    text(ui, "Element namespace", &mut element.namespace);
                    text(ui, "Element local name", &mut element.local);
                    optional_text(ui, "Element prefix", &mut element.prefix);
                    bindings_ui(ui, &mut element.namespaces);
                    ui.collapsing("Attributes", |ui| {
                        let mut remove = None;
                        for (j, attribute) in element.attributes.iter_mut().enumerate() {
                            ui.push_id(j, |ui| {
                                text(ui, "Attribute namespace", &mut attribute.namespace);
                                text(ui, "Attribute local name", &mut attribute.local);
                                optional_text(ui, "Attribute prefix", &mut attribute.prefix);
                                text(ui, "Value", &mut attribute.value);
                                if ui.button("Remove attribute").clicked() {
                                    remove = Some(j);
                                }
                            });
                        }
                        if let Some(j) = remove {
                            element.attributes.remove(j);
                        }
                        if ui
                            .add_enabled(
                                element.attributes.len() < 256,
                                egui::Button::new("Add attribute"),
                            )
                            .clicked()
                        {
                            element.attributes.push(AttributeDraft {
                                namespace: String::new(),
                                local: String::new(),
                                prefix: None,
                                value: String::new(),
                            });
                        }
                    });
                    if depth < 31 {
                        ui.collapsing("Child content", |ui| {
                            nodes_ui(ui, &mut element.children, depth + 1)
                        });
                    } else {
                        ui.label("Further nesting is unavailable.");
                    }
                }
            };
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(i > 0, egui::Button::new("Move up"))
                    .clicked()
                {
                    move_item = Some((i, i - 1));
                }
                if ui.button("Remove content").clicked() {
                    remove = Some(i);
                }
            });
        });
    }
    if let Some(i) = remove {
        nodes.remove(i);
    } else if let Some((a, b)) = move_item {
        nodes.swap(a, b);
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(nodes.len() < 256, egui::Button::new("Add text"))
            .clicked()
        {
            nodes.push(NodeDraft::Text(String::new()));
        }
        if ui
            .add_enabled(
                depth < 31 && nodes.len() < 256,
                egui::Button::new("Add element"),
            )
            .clicked()
        {
            nodes.push(NodeDraft::Element(ElementDraft {
                namespace: String::new(),
                local: String::new(),
                prefix: None,
                namespaces: Vec::new(),
                attributes: Vec::new(),
                children: Vec::new(),
            }));
        }
        if ui
            .add_enabled(nodes.len() < 256, egui::Button::new("Add comment"))
            .clicked()
        {
            nodes.push(NodeDraft::Comment(String::new()));
        }
        if ui
            .add_enabled(nodes.len() < 256, egui::Button::new("Add instruction"))
            .clicked()
        {
            nodes.push(NodeDraft::Instruction {
                target: String::new(),
                data: None,
            });
        }
    });
}

pub(super) enum ExpressionBodyDraft {
    Text(String),
    Mixed(Vec<ExpressionPartDraft>),
}
pub(super) enum ExpressionPartDraft {
    Text(String),
    Documentation(super::documentation::EntryDraft),
    Extension(ContentDraft),
    Comment(String),
    Instruction(String, Option<String>),
}
impl ExpressionBodyDraft {
    pub(super) fn new(body: &processcraft_core::bpmn_model::ExpressionBody) -> Self {
        use processcraft_core::bpmn_model::{ExpressionBody, ExpressionPart};
        match body {
            ExpressionBody::Text(text) => Self::Text(text.clone()),
            ExpressionBody::Mixed { parts } => Self::Mixed(
                parts
                    .iter()
                    .map(|p| match p {
                        ExpressionPart::Text { text } => ExpressionPartDraft::Text(text.clone()),
                        ExpressionPart::Documentation { documentation } => {
                            ExpressionPartDraft::Documentation(
                                super::documentation::EntryDraft::new(documentation),
                            )
                        }
                        ExpressionPart::ExtensionElements { extension_elements } => {
                            ExpressionPartDraft::Extension(ContentDraft::new(
                                &extension_elements.content,
                            ))
                        }
                        ExpressionPart::Comment { text } => {
                            ExpressionPartDraft::Comment(text.clone())
                        }
                        ExpressionPart::ProcessingInstruction { target, data } => {
                            ExpressionPartDraft::Instruction(target.to_string(), data.clone())
                        }
                    })
                    .collect(),
            ),
        }
    }
    pub(super) fn build(&self) -> Result<processcraft_core::bpmn_model::ExpressionBody, String> {
        use processcraft_core::bpmn_model::{ExpressionBody, ExpressionPart};
        Ok(match self {
            Self::Text(text) => ExpressionBody::Text(text.clone()),
            Self::Mixed(parts) => ExpressionBody::Mixed {
                parts: parts
                    .iter()
                    .map(|p| {
                        Ok(match p {
                            ExpressionPartDraft::Text(text) => {
                                ExpressionPart::Text { text: text.clone() }
                            }
                            ExpressionPartDraft::Documentation(doc) => {
                                ExpressionPart::Documentation {
                                    documentation: doc.build()?,
                                }
                            }
                            ExpressionPartDraft::Extension(content) => {
                                ExpressionPart::ExtensionElements {
                                    extension_elements:
                                        processcraft_core::bpmn_metadata::ExtensionElements {
                                            content: content.build()?,
                                        },
                                }
                            }
                            ExpressionPartDraft::Comment(text) => {
                                ExpressionPart::Comment { text: text.clone() }
                            }
                            ExpressionPartDraft::Instruction(target, data) => {
                                ExpressionPart::ProcessingInstruction {
                                    target: target.clone(),
                                    data: data.clone(),
                                }
                            }
                        })
                    })
                    .collect::<Result<_, String>>()?,
            },
        })
    }
    pub(super) fn ui(&mut self, ui: &mut egui::Ui) {
        match self {
            Self::Text(text) => {
                let label = ui.label("Expression text");
                ui.text_edit_multiline(text).labelled_by(label.id);
                if ui.button("Add mixed content").clicked() {
                    *self = Self::Mixed(vec![ExpressionPartDraft::Text(text.clone())]);
                }
            }
            Self::Mixed(parts) => {
                let mut remove = None;
                let mut up = None;
                for (i, part) in parts.iter_mut().enumerate() {
                    ui.push_id(i, |ui| {
                        ui.separator();
                        match part {
                            ExpressionPartDraft::Text(value) => {
                                let label = ui.label("Text");
                                ui.text_edit_multiline(value).labelled_by(label.id);
                            }
                            ExpressionPartDraft::Documentation(value) => {
                                ui.label("Documentation");
                                value.ui(ui);
                            }
                            ExpressionPartDraft::Extension(value) => {
                                ui.label("Extension elements");
                                value.ui(ui, 0);
                            }
                            ExpressionPartDraft::Comment(value) => {
                                text(ui, "Comment", value);
                            }
                            ExpressionPartDraft::Instruction(target, data) => {
                                text(ui, "Instruction target", target);
                                optional_text(ui, "Instruction data", data);
                            }
                        }
                        if ui
                            .add_enabled(i > 0, egui::Button::new("Move up"))
                            .clicked()
                        {
                            up = Some(i);
                        }
                        if ui.button("Remove part").clicked() {
                            remove = Some(i);
                        }
                    });
                }
                if let Some(i) = remove {
                    parts.remove(i);
                } else if let Some(i) = up {
                    parts.swap(i, i - 1);
                }
                ui.add_enabled_ui(parts.len() < 256, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Add text").clicked() {
                            parts.push(ExpressionPartDraft::Text(String::new()));
                        }
                        if ui.button("Add documentation").clicked() {
                            parts.push(ExpressionPartDraft::Documentation(
                                super::documentation::EntryDraft::empty(),
                            ));
                        }
                        if ui.button("Add extension").clicked() {
                            parts.push(ExpressionPartDraft::Extension(ContentDraft::new(
                                &XmlContent {
                                    namespaces: vec![],
                                    nodes: vec![],
                                },
                            )));
                        }
                        if ui.button("Add comment").clicked() {
                            parts.push(ExpressionPartDraft::Comment(String::new()));
                        }
                        if ui.button("Add instruction").clicked() {
                            parts.push(ExpressionPartDraft::Instruction(String::new(), None));
                        }
                    });
                });
            }
        }
    }
}

impl AttributeDraft {
    pub(super) fn new(a: &XmlAttribute) -> Self {
        Self {
            namespace: a.namespace.clone(),
            local: a.local.to_string(),
            prefix: a.prefix.as_ref().map(ToString::to_string),
            value: a.value.clone(),
        }
    }
    pub(super) fn build(&self) -> Result<XmlAttribute, String> {
        Ok(XmlAttribute {
            namespace: self.namespace.clone(),
            local: xml_name(&self.local)?,
            prefix: optional_name(&self.prefix)?,
            value: self.value.clone(),
        })
    }
    pub(super) fn ui(&mut self, ui: &mut egui::Ui) {
        text(ui, "Attribute namespace", &mut self.namespace);
        text(ui, "Attribute local name", &mut self.local);
        optional_text(ui, "Attribute prefix", &mut self.prefix);
        text(ui, "Attribute value", &mut self.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn structured_content_round_trips_order_and_invalid_names_remain_drafts() {
        let original = MixedXml::Structured(XmlContent {
            namespaces: vec![NamespaceBinding {
                prefix: "p".into(),
                uri: "urn:test".into(),
            }],
            nodes: vec![
                XmlNode::Text {
                    text: "before".into(),
                },
                XmlNode::Element {
                    element: XmlElement {
                        namespace: "urn:test".into(),
                        local: "item".parse().unwrap(),
                        prefix: Some("p".parse().unwrap()),
                        namespaces: vec![],
                        attributes: vec![],
                        children: vec![XmlNode::Comment {
                            text: "note".into(),
                        }],
                    },
                },
                XmlNode::Text {
                    text: "after".into(),
                },
            ],
        });
        let mut draft = MixedDraft::new(&original);
        assert_eq!(draft.build().unwrap(), original);
        if let MixedDraft::Structured(content) = &mut draft
            && let NodeDraft::Element(element) = &mut content.nodes[1]
        {
            element.local = "bad name".into();
        }
        assert!(draft.build().is_err());
        if let MixedDraft::Structured(content) = &draft {
            if let NodeDraft::Element(element) = &content.nodes[1] {
                assert_eq!(element.local, "bad name");
            } else {
                panic!("element lost");
            }
        } else {
            panic!("structured content lost");
        }
    }
}
