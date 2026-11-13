const BPMN: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<definitions xmlns="http://www.omg.org/spec/BPMN/20100524/MODEL" xmlns:bpmndi="http://www.omg.org/spec/BPMN/20100524/DI" xmlns:dc="http://www.omg.org/spec/DD/20100524/DC" xmlns:di="http://www.omg.org/spec/DD/20100524/DI" id="defs" targetNamespace="urn:test">
  <process id="p" isExecutable="false">
    <startEvent id="start"><messageEventDefinition id="md"/></startEvent>
    <userTask id="task" name="Check &lt;A&gt; &amp; B"/>
    <boundaryEvent id="timer" attachedToRef="task" cancelActivity="false"><timerEventDefinition id="td"/></boundaryEvent>
    <exclusiveGateway id="gw" default="f3"/>
    <subProcess id="sub"/>
    <endEvent id="end"/>
    <sequenceFlow id="f1" sourceRef="start" targetRef="task"/>
    <sequenceFlow id="f2" sourceRef="task" targetRef="gw"/>
    <sequenceFlow id="f3" sourceRef="gw" targetRef="sub"/>
    <sequenceFlow id="f4" sourceRef="sub" targetRef="end"/>
  </process>
  <bpmndi:BPMNDiagram id="d">
    <bpmndi:BPMNPlane id="plane" bpmnElement="p">
      <bpmndi:BPMNShape id="start_di" bpmnElement="start"><dc:Bounds x="0" y="22" width="36" height="36"/></bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="task_di" bpmnElement="task"><dc:Bounds x="80" y="0" width="100" height="80"/></bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="timer_di" bpmnElement="timer"><dc:Bounds x="112" y="62" width="36" height="36"/></bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="gw_di" bpmnElement="gw"><dc:Bounds x="220" y="15" width="50" height="50"/></bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="sub_di" bpmnElement="sub" isExpanded="false"><dc:Bounds x="310" y="0" width="100" height="80"/></bpmndi:BPMNShape>
      <bpmndi:BPMNShape id="end_di" bpmnElement="end"><dc:Bounds x="450" y="22" width="36" height="36"/></bpmndi:BPMNShape>
      <bpmndi:BPMNEdge id="f1_di" bpmnElement="f1"><di:waypoint x="36" y="40"/><di:waypoint x="80" y="40"/></bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="f2_di" bpmnElement="f2"><di:waypoint x="180" y="40"/><di:waypoint x="220" y="40"/></bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="f3_di" bpmnElement="f3"><di:waypoint x="270" y="40"/><di:waypoint x="290" y="40"/><di:waypoint x="310" y="40"/></bpmndi:BPMNEdge>
      <bpmndi:BPMNEdge id="f4_di" bpmnElement="f4"><di:waypoint x="410" y="40"/><di:waypoint x="450" y="40"/></bpmndi:BPMNEdge>
    </bpmndi:BPMNPlane>
  </bpmndi:BPMNDiagram>
</definitions>"#;

#[test]
fn diagram_exports_as_native_drawio_bpmn_shapes() {
    let document = processcraft_io::interchange::prepare_import(BPMN.as_bytes(), "drawio")
        .unwrap()
        .document;
    let out = processcraft_io::drawio::export_drawio(&document, &"d".parse().unwrap()).unwrap();
    let xml = String::from_utf8(out.bytes).unwrap();
    assert!(out.approximated.is_empty(), "{:?}", out.approximated);
    assert!(xml.contains(r#"<mxCell id="0"/><mxCell id="1" parent="0"/>"#));
    for expected in [
        "outline=standard;symbol=message;",
        "taskMarker=user;",
        "outline=boundNonint;symbol=timer;",
        "gwType=exclusive;",
        "isLoopSub=1;",
        "outline=end;symbol=general;",
        // Default flow marker, and the bend point kept between source and target.
        r#"startArrow=dash;startFill=0;startSize=6;" edge="1" parent="1" source="s3" target="s4""#,
        r#"<Array as="points"><mxPoint x="290" y="40"/></Array>"#,
        // HTML label inside an XML attribute: escaped twice.
        r#"value="Check &amp;lt;A&amp;gt; &amp;amp; B""#,
    ] {
        assert!(xml.contains(expected), "missing {expected}\n{xml}");
    }
}
