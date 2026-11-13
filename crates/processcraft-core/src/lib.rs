//! Toolkit-independent process identities and persistent model.
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

pub mod arrange;
pub mod attributes;
pub mod clipboard;
pub mod commands;
pub mod containers;
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
