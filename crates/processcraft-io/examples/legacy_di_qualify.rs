//! Qualify the historical native implicit-label bridge in a named artifact directory.
use processcraft_core::model::{Bounds, Label, ProjectData};
use processcraft_io::{interchange, interchange_compare::xml_model_value, native};
use serde_json::json;
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("new absolute output directory required")?,
    );
    if !output.is_absolute() || output.exists() {
        return Err("output must be a new absolute directory".into());
    }
    let input = include_bytes!("../../../tests/fixtures/bpmn/interop-process.bpmn");
    let imported = interchange::prepare_import(input, "Legacy native DI preparation")?;
    if !imported.report.editable {
        return Err("fixture is protected".into());
    }
    let mut document = imported.document;
    // Prepare a native-only historical state deliberately, without claiming that
    // the original XML requested visible labels. The third shape and edges retain
    // their explicitly absent labels and must stay hidden in the output.
    document.diagrams[0].shapes[0].label_visibility = None;
    document.diagrams[0].shapes[0].label = None;
    document.diagrams[0].shapes[1].label_visibility = None;
    document.diagrams[0].shapes[1].label = Some(Label {
        di_metadata: None,
        bounds: Some(Bounds {
            x: 210.0,
            y: 170.0,
            width: 100.0,
            height: 25.0,
        }),
        ..Label::default()
    });
    let id = document.id;
    let mut project = ProjectData::new("Legacy DI qualification".into());
    project.documents.push(Arc::new(document));
    project.resources = imported.resources;
    project.validate()?;
    let before = project.content_identity()?;
    let archive = native::write_bytes(&project)?;
    let reopened = native::load_bytes(&archive)?.project;
    if reopened.content_identity()? != before {
        return Err("native identity changed".into());
    }
    let exported = interchange::export_bpmn(&reopened, id, false)
        .map_err(|error| format!("export refused: {:?}", error.report.blocked))?;
    let again = interchange::prepare_import(&exported.bytes, "Reimport")?;
    if !again.report.editable
        || xml_model_value(&again.document)? != xml_model_value(&project.documents[0])?
    {
        return Err("legacy label meaning changed across interchange".into());
    }
    let labels = &again.document.diagrams[0];
    if labels.shapes[0]
        .label
        .as_ref()
        .is_none_or(|label| label.bounds.is_some() || label.id.is_some())
        || labels.shapes[1]
            .label
            .as_ref()
            .and_then(|label| label.bounds.as_ref())
            != reopened.documents[0].diagrams[0].shapes[1]
                .label
                .as_ref()
                .and_then(|label| label.bounds.as_ref())
        || labels.shapes[2].label.is_some()
        || labels.edges.iter().any(|edge| edge.label.is_some())
    {
        return Err("label presence, geometry or optional identity changed".into());
    }
    if project.content_identity()? != before {
        return Err("export mutated native content".into());
    }
    std::fs::create_dir(&output)?;
    std::fs::write(output.join("source-basis.bpmn"), input)?;
    std::fs::write(output.join("prepared-native.processcraft"), archive)?;
    std::fs::write(output.join("output.bpmn"), exported.bytes)?;
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&json!({
            "preparation":"two native shapes use historical implicit visibility; one retains explicit label geometry; remaining labels explicitly hidden",
            "nativeIdentityPreserved":true,"xmlMeaningPreserved":true,"labelIdsInvented":false,
            "transformed":exported.report.transformed,"omitted":exported.report.omitted,
            "limits":"in-memory native codec and explicit artifact writes; not a Windows NTFS save or recovery qualification"
        }))?,
    )?;
    println!(
        "Legacy visible labels, explicit hidden labels and native identity preserved: {}",
        output.display()
    );
    Ok(())
}
