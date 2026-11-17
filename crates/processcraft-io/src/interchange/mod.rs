//! Conservative BPMN interchange: validate first, then qualify a small lossless profile.
//!
//! Import (`prepare_import`) parses the source safely, projects only content inside a qualified
//! profile into the typed model, and re-exports that projection to confirm it round-trips
//! (qualify, export, re-parse, compare via `interchange_compare`). Anything that fails the loop
//! stays as non-editable preserved source: content is preserved byte-for-byte or refused,
//! never silently dropped or reinterpreted. `export_bpmn` applies the same checks on the way out.
//!
//! Layout: `import/` turns XML into the typed model, `export/` writes it back, `shared.rs` holds
//! what both need and `refusal.rs` records why an import was refused. `attributes` handles the
//! `pc:attributes` extension. This file keeps only the public API.
mod attributes;
mod export;
mod import;
mod refusal;
mod shared;
use crate::bpmn::{ImportError, import_bpmn};
use crate::interchange::export::document::export_document;
use crate::interchange::import::profile::{invalid_choreography_loop_types, parse_profile};
use crate::interchange::refusal::{REFUSAL, leave, refused};
pub use attributes::{
    AttributeImportPlan, ImportedAttributeValue, ImportedAttributes, plan_attribute_import,
};
use processcraft_core::ids::XmlId;
use processcraft_core::model::*;
use std::sync::Arc;
/// Namespace of the custom attribute values written to `extensionElements` (M5.1b).
pub const PC_ATTRIBUTES_NS: &str = "urn:processcraft:attributes:1";
#[derive(Debug)]
pub struct ImportReport {
    pub editable: bool,
    pub limitations: Vec<String>,
}
#[derive(Debug)]
pub struct PreparedImport {
    pub document: BpmnDocument,
    pub resources: Vec<Resource>,
    pub report: ImportReport,
    /// Well-formed `pc:attributes` lifted out of the document's extension content.
    pub attributes: ImportedAttributes,
}
/// No mutation or filesystem access occurs until the caller commits this bundle.
pub fn prepare_import(bytes: &[u8], source_name: &str) -> Result<PreparedImport, ImportError> {
    import_bpmn(bytes)?;
    let rid: XmlId = format!("source_{}", uuid::Uuid::new_v4().simple())
        .parse()
        .expect("generated ID");
    let resource = Resource::new(rid.clone(), "application/xml".into(), Arc::from(bytes));
    REFUSAL.with(|r| r.take());
    leave();
    let initial = parse_profile(bytes);
    if initial.is_none() {
        refused::<()>(|| "content outside the qualified profile".into());
    }
    leave();
    let projected = initial.and_then(|document| {
        let check = (document.has_semantic_common_metadata_features()
            || document.has_di_choreography_features()
            || document.has_expanded_collaboration()
            || document.choreographies.iter().any(|c| {
                c.kind.is_some()
                    || c.graph.is_some()
                    || c.activities
                        .iter()
                        .any(|a| a.loop_type.is_some() || a.details.is_some())
            }))
        .then(|| export_document(&document, std::slice::from_ref(&resource), true, None));
        if let Some(Err(failure)) = check {
            // The export check re-parses its own output; its breadcrumbs are not the reason.
            REFUSAL.with(|r| r.take());
            leave();
            refused(|| {
                let detail = failure.report.blocked.join("; ");
                format!("the qualified model does not re-export losslessly: {detail}")
            })
        } else {
            Some(document)
        }
    });
    let mut doc = projected.unwrap_or_else(|| {
        let mut d = BpmnDocument::new(
            source_name.into(),
            "preserved_definitions".parse().expect("constant"),
        );
        d.capability = Capability::PreservedSource {
            source_resource_id: rid.clone(),
        };
        d
    });
    doc.name = source_name.into();
    doc.provenance = Some(Provenance {
        source_resource_id: rid,
        source_name: source_name.into(),
    });
    let editable = matches!(doc.capability, Capability::Editable);
    let mut attribute_limitations = Vec::new();
    let attributes = if editable {
        attributes::lift(&mut doc, &mut attribute_limitations)
    } else {
        ImportedAttributes::default()
    };
    Ok(PreparedImport {
        document: doc,
        resources: vec![resource],
        attributes,
        report: ImportReport {
            editable,
            limitations: if editable {
                let mut limitations =
                    vec!["Qualified BPMN subset; original source retained".into()];
                limitations.extend(attribute_limitations);
                limitations
            } else {
                let reason = REFUSAL
                    .with(|r| r.take())
                    .unwrap_or_else(|| "content outside the qualified profile".into());
                let mut limitations = vec![format!(
                    "Semantic editing disabled ({reason}); source retained byte-for-byte"
                )];
                limitations.extend(invalid_choreography_loop_types(bytes));
                limitations
            },
        },
    })
}
#[derive(Debug, Default)]
pub struct ExportReport {
    pub retained: Vec<String>,
    pub transformed: Vec<String>,
    pub omitted: Vec<String>,
    pub blocked: Vec<String>,
}
#[derive(Debug)]
pub struct ExportedBpmn {
    pub bytes: Vec<u8>,
    pub report: ExportReport,
}
#[derive(Debug)]
pub struct ExportFailure {
    pub report: ExportReport,
}
pub fn export_bpmn(
    project: &ProjectData,
    document_id: processcraft_core::ids::DocumentId,
    acknowledge_native_fields: bool,
) -> Result<ExportedBpmn, ExportFailure> {
    let Some(document) = project.documents.iter().find(|d| d.id == document_id) else {
        return Err(ExportFailure {
            report: ExportReport {
                blocked: vec!["Document not found".into()],
                ..ExportReport::default()
            },
        });
    };
    let has_links = project
        .links
        .iter()
        .any(|l| l.source.document_id == document_id)
        || project
            .dictionary_links
            .iter()
            .any(|l| l.source.document_id == document_id);
    if has_links && !acknowledge_native_fields {
        return Err(ExportFailure {
            report: ExportReport {
                omitted: vec![
                    "Project navigation links and dictionary links are native-only".into(),
                ],
                blocked: vec!["Native-only link omission requires acknowledgement".into()],
                ..ExportReport::default()
            },
        });
    }
    if let Err(error) = project.validate() {
        return Err(ExportFailure {
            report: ExportReport {
                blocked: vec![format!("Invalid project: {error}")],
                ..ExportReport::default()
            },
        });
    }
    let values: Vec<_> = project
        .attribute_values
        .iter()
        .filter(|v| v.source.document_id == document_id)
        .collect();
    let attributes = attributes::ExportAttributes {
        definitions: &project.attribute_definitions,
        entries: &project.dictionary_entries,
        values,
    };
    let mut result = export_document(
        document,
        &project.resources,
        acknowledge_native_fields,
        Some(&attributes),
    )?;
    if has_links {
        result
            .report
            .omitted
            .push("Project navigation links and dictionary links are native-only".into());
    }
    if !project.attribute_definitions.is_empty() {
        // Informational, like the document folder: values carry no `required`,
        // `applies_to`, description or unused enum options.
        result.report.omitted.push(
            "Attribute definition metadata (description, required, applies_to, unused enum options) and definitions without values are native-only"
                .into(),
        );
    }
    Ok(result)
}
