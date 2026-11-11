//! Produce an actual native archive for independent ZIP/JSON inspection.
use processcraft_core::model::{BpmnDocument, ProjectData, Provenance, Resource};
use processcraft_io::native::write_bytes;
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = match args.next() {
        None => PathBuf::from("target/m1-native/sample.processcraft"),
        Some(flag) if flag == "--out" => PathBuf::from(args.next().ok_or("missing --out path")?),
        _ => return Err("usage: native_sample [--out PATH]".into()),
    };
    if args.next().is_some() {
        return Err("unexpected arguments".into());
    }
    let mut project = ProjectData::new("Native sample".into());
    let resource_id = "source".parse()?;
    let source: Arc<[u8]> =
        Arc::from(&b"<definitions xmlns=\"http://www.omg.org/spec/BPMN/20100524/MODEL\"/>"[..]);
    project
        .resources
        .push(Resource::new(resource_id, "application/xml".into(), source));
    let mut document = BpmnDocument::new("Example".into(), "definitions".parse()?);
    document.provenance = Some(Provenance {
        source_resource_id: "source".parse()?,
        source_name: "example.bpmn".into(),
    });
    project.documents.push(Arc::new(document));
    let bytes = write_bytes(&project)?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, bytes)?;
    println!("{}", path.display());
    Ok(())
}
