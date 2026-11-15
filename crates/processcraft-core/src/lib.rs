//! Toolkit-independent BPMN domain core: no filesystem, GUI, dialogs or networking.
//!
//! Module groups:
//! - model: `ids`, `model`, `bpmn_*` (typed BPMN content), `metadata_read` / `metadata_visit`.
//! - command pipeline: `commands` (the only way to change a project), `history`, `rules`,
//!   `modeling`, `replace`, `manipulate`, `containers`, `clipboard`, `arrange`.
//! - validity vs diagnostics: hard structural validity lives in `identity`
//!   (`ProjectData::validate`); non-blocking editing diagnostics live in `validation*` and
//!   `conventions`.
//! - geometry and layout: `geometry`, `layout`.
//! - analyst layer: `attributes`, `conventions`, `overlay`, and the read-only `queries`.
//!
//! See `docs/design/solution-design.md` for the architecture.
mod element_identity_edit;
pub mod field_projection;
mod identity;
pub mod ids;
pub mod metadata_read;
pub mod metadata_visit;
pub mod model;
pub mod queries;
pub mod reference_resolution;
pub use ids::{DictionaryEntryId, DocumentId, InvalidId, ProjectId};

/// Most checkpoints a project may hold (command, validation and native load all enforce it).
pub const MAX_CHECKPOINTS: usize = 20;

pub mod arrange;
pub mod attributes;
pub mod clipboard;
pub mod commands;
pub mod containers;
pub mod conventions;
pub mod geometry;
mod history;

pub mod validation;
pub mod validation_data;
pub mod validation_flow;
pub use validation::{Diagnostic, DiagnosticSeverity};

pub mod layout;
pub mod manipulate;
pub mod modeling;
pub mod overlay;
pub mod replace;
pub mod rules;

pub mod bpmn_catalog;
pub mod bpmn_metadata;
pub mod bpmn_model;
pub mod bpmn_xml;

pub mod bpmn_scope;

pub mod bpmn_collaboration;

pub mod bpmn_data;

mod activity_read;
mod base_identity;
pub mod bpmn_activity;
mod collaboration_read;
