//! Qualify an explicit local XML fixture through native persistence and interchange.
//! This tool reads only its named input and writes only its named output directory.
use processcraft_core::{model::ProjectData, queries};
use processcraft_io::{interchange, native};
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

fn fields(project: &ProjectData) -> BTreeMap<String, serde_json::Value> {
    queries::document_fields(project, project.documents[0].id)
        .expect("document exists")
        .into_iter()
        .filter(|field| {
            !["/id", "/name", "/provenance", "/capability"]
                .iter()
                .any(|prefix| {
                    field.path == *prefix || field.path.starts_with(&format!("{prefix}/"))
                })
        })
        .map(|field| (field.path, field.value))
        .collect()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("input BPMN file required")?,
    );
    let output = PathBuf::from(
        std::env::args_os()
            .nth(2)
            .ok_or("output directory required")?,
    );
    if !output.is_absolute() || output.exists() {
        return Err("output must be a new absolute directory".into());
    }
    let source = std::fs::read(&input)?;
    let imported = interchange::prepare_import(&source, "Common metadata qualification")?;
    if !imported.report.editable {
        return Err("source is protected, not qualified for editing".into());
    }
    let id = imported.document.id;
    let mut project = ProjectData::new("Common metadata qualification".into());
    project.documents.push(Arc::new(imported.document));
    project.resources = imported.resources;
    project.validate()?;
    let expected = fields(&project);
    let before = project.content_identity()?;
    let archive = native::write_bytes(&project)?;
    let reopened = native::load_bytes(&archive)?.project;
    if reopened.content_identity()? != before || fields(&reopened) != expected {
        return Err("native identity or model fields changed".into());
    }
    let exported = interchange::export_bpmn(&reopened, id, false)
        .map_err(|error| format!("export refused: {:?}", error.report.blocked))?;
    let again = interchange::prepare_import(&exported.bytes, "Reimported qualification")?;
    if !again.report.editable {
        return Err("exported XML is protected on reimport".into());
    }
    let mut comparison = ProjectData::new("Comparison".into());
    comparison.documents.push(Arc::new(again.document));
    comparison.resources = again.resources;
    comparison.validate()?;
    let actual = fields(&comparison);
    let paths: std::collections::BTreeSet<_> = expected.keys().chain(actual.keys()).collect();
    let differences: Vec<_> = paths
        .into_iter()
        .filter(|path| expected.get(*path) != actual.get(*path))
        .map(|path| json!({"path":path,"before":expected.get(path),"after":actual.get(path)}))
        .collect();
    std::fs::create_dir(&output)?;
    std::fs::write(output.join("input.bpmn"), source)?;
    std::fs::write(output.join("output.bpmn"), exported.bytes)?;
    std::fs::write(output.join("fixture.processcraft"), archive)?;
    std::fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&json!({
            "nativeIdentityPreserved":true,"fieldDifferences":differences
        }))?,
    )?;
    if !differences.is_empty() {
        return Err(format!(
            "{} modeled field differences; inspect comparison.json",
            differences.len()
        )
        .into());
    }
    println!(
        "Native identity and all modeled fields preserved: {}",
        output.display()
    );
    Ok(())
}
