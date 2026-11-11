//! Test-only preservation harness. Explicit output directory supplied by caller.
use processcraft_io::bpmn::import_bpmn;
use processcraft_io::{Access, FileGrant, OpenSession, SaveOutcome, WriterSession};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("absolute output directory required")?,
    );
    if !output.is_absolute() {
        return Err("absolute output directory required".into());
    }
    std::fs::create_dir_all(&output)?;
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/bpmn");
    for name in [
        "minimal",
        "extensions",
        "multiple-diagrams",
        "unsupported-event",
        "namespace-rebinding",
    ] {
        let input = std::fs::read(fixtures.join(format!("{name}.bpmn")))?;
        let document = import_bpmn(&input)?;
        let grant = FileGrant::new(output.join(format!("{name}.bpmn")), Access::Write)?;
        let OpenSession::Writable(mut writer) = WriterSession::open(&grant)? else {
            return Err("output is not writable".into());
        };
        match writer.save(document.export_source()) {
            SaveOutcome::Committed { warning: None } => {}
            outcome => {
                return Err(format!("preserved output did not commit cleanly: {outcome:?}").into());
            }
        }
        assert_eq!(std::fs::read(output.join(format!("{name}.bpmn")))?, input);
        println!(
            "{name}: preserved original bytes, {} diagrams, semantic edits disabled",
            document.report().diagram_count
        );
    }
    Ok(())
}
