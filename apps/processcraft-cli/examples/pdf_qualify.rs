//! Reproducible PDF/PNG scene corpus for independent qualification.
use processcraft_core::model::{Bounds, BpmnDocument, Diagram, Node, NodeKind, Process, Shape};
use processcraft_render::{Appearance, ExportFormat, ExportOptions, TextEngine, export_diagram};
use std::{fs, path::Path};
fn id<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().expect("valid fixture id")
}
fn sample(tiled: bool) -> BpmnDocument {
    let mut d = BpmnDocument::new("PDF qualification".into(), id("defs"));
    let labels = [
        "Caffè e qualità",
        "مراجعة الطلب",
        "采购流程",
        "A\u{301} fin de revisión",
    ];
    let mut nodes = Vec::new();
    let mut shapes = Vec::new();
    for (i, name) in labels.iter().enumerate() {
        let node: processcraft_core::ids::ElementIdentity = id(&format!("node{i}"));
        nodes.push(Node {
            flow_metadata: None,
            base_extras: None,
            item_aware: None,
            activity: None,
            io_specification: None,
            loop_characteristics: None,
            id: node.clone(),
            name: Some((*name).into()),
            documentation: String::new().into(),
            kind: NodeKind::Task,
            details: None,
            owner: String::new(),
            contact: String::new(),
        });
        shapes.push(Shape {
            label_visibility: None,
            is_horizontal: None,
            is_marker_visible: None,
            is_expanded: None,
            id: id(&format!("shape{i}")),
            element: node.selector(),
            bounds: Bounds {
                x: if tiled {
                    i as f64 * 600.0
                } else {
                    i as f64 * 190.0
                },
                y: if tiled && i % 2 == 1 { 700.0 } else { 0.0 },
                width: 170.0,
                height: 80.0,
            },
            label: None,
            style: None,
        });
    }
    d.processes.push(Process {
        activity_metadata: None,
        base_extras: None,
        is_executable: Some(false),
        io_specification: None,
        lane_set_id: None,
        lane_set_base: None,
        groups: vec![],
        data_objects: vec![],
        data_associations: vec![],
        id: id("process"),
        name: Some(String::new()),
        documentation: String::new().into(),
        nodes,
        lanes: vec![],
        flows: vec![],
    });
    d.diagrams.push(Diagram {
        documentation: None,
        resolution: None,
        label_styles: vec![],
        name: None,
        id: id("diagram"),
        plane_id: id("plane"),
        plane: id("process"),
        shapes,
        edges: vec![],
    });
    d
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = Path::new("target/m3-qualification/pdf");
    fs::create_dir_all(out)?;
    let mut report = Vec::new();
    for (name, tiled, appearance) in [
        ("light", false, Appearance::Light),
        ("dark", false, Appearance::Dark),
        ("tiled", true, Appearance::Light),
    ] {
        let d = sample(tiled);
        let mut fonts = TextEngine::new();
        let opts = ExportOptions {
            format: ExportFormat::Pdf,
            appearance,
            scale: 1.0,
            acknowledge_diagnostics: false,
        };
        let refusal = export_diagram(&d, &id("diagram"), opts, &mut fonts, "Noto Sans");
        if refusal.is_ok() {
            return Err("PDF exported without mandatory acknowledgement".into());
        }
        let pdf = export_diagram(
            &d,
            &id("diagram"),
            ExportOptions {
                acknowledge_diagnostics: true,
                ..opts
            },
            &mut fonts,
            "Noto Sans",
        )?;
        let png = export_diagram(
            &d,
            &id("diagram"),
            ExportOptions {
                format: ExportFormat::Png,
                acknowledge_diagnostics: true,
                ..opts
            },
            &mut fonts,
            "Noto Sans",
        )?;
        fs::write(out.join(format!("{name}.pdf")), &pdf.bytes)?;
        fs::write(out.join(format!("{name}.png")), &png.bytes)?;
        report.push(serde_json::json!({"name":name,"logical_size":pdf.logical_size,"pixel_size":png.pixel_size,"origin":pdf.origin,"diagnostics":pdf.diagnostics,"ack_refused":true}));
        println!("PDF_QUALIFY {name} generated");
    }
    let mut huge = sample(true);
    huge.diagrams[0].shapes[3].bounds.x = 150_000.0;
    let mut fonts = TextEngine::new();
    let capped = export_diagram(
        &huge,
        &id("diagram"),
        ExportOptions {
            format: ExportFormat::Pdf,
            appearance: Appearance::Light,
            scale: 8.0,
            acknowledge_diagnostics: true,
        },
        &mut fonts,
        "Noto Sans",
    );
    if capped.is_ok() {
        return Err("PDF page cap did not reject oversized export".into());
    }
    fs::write(
        out.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"samples":report,"page_cap_refused":true}))?,
    )?;
    println!("PDF_QUALIFY generation complete");
    Ok(())
}
