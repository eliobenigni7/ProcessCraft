//! XML namespace URIs shared by the BPMN reader, interchange and exporters.

/// BPMN 2.0 semantic model (`bpmn:` prefix).
pub(crate) const BPMN: &str = "http://www.omg.org/spec/BPMN/20100524/MODEL";
/// BPMN 2.0 diagram interchange (`bpmndi:` prefix).
pub(crate) const BPMNDI: &str = "http://www.omg.org/spec/BPMN/20100524/DI";
/// OMG DD common types: `Bounds`, `Point`, `Font` (`dc:` prefix).
pub(crate) const DC: &str = "http://www.omg.org/spec/DD/20100524/DC";
/// OMG DD diagram interchange: `waypoint`, `extension` (`di:` prefix).
pub(crate) const DI: &str = "http://www.omg.org/spec/DD/20100524/DI";
/// XML Schema instance (`xsi:` prefix).
pub(crate) const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
/// Reserved `xml:` prefix namespace.
pub(crate) const XML: &str = "http://www.w3.org/XML/1998/namespace";
/// Reserved `xmlns:` prefix namespace.
pub(crate) const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
