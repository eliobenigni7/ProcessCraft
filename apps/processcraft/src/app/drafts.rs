//! The open property drafts of the inspector panels, enumerated in exactly one place.
//!
//! Each panel keeps its half-edited state in one `Option` slot here. The macro below is the single
//! list: it generates the struct, `any`, `clear`, the lock query and the ordered commit table, so
//! a new panel is added by one line. Everything that is not a plain draft slot (simple-field
//! drafts, attribute cells, collaboration/review/dictionary drafts) stays on `ProcessCraft` and is
//! combined with these in `ProcessCraft::has_drafts` and `ProcessCraft::select`.
use super::ProcessCraft;

/// Applies one open draft through the controller (see `ProcessCraft::apply_typed_draft`).
pub(super) type Apply = fn(&mut ProcessCraft) -> Result<(), String>;

macro_rules! drafts {
    (
        // Finished by hand in its own panel; never applied by the commit table.
        manual { $mf:ident: $mt:ty }
        // `locks`: while open, the ordinary (simple-field) property editors are disabled.
        typed { $($f:ident: $t:ty => $label:literal, $apply:ident, locks: $locks:literal;)* }
    ) => {
        #[derive(Default)]
        pub(super) struct Drafts {
            pub(super) $mf: Option<$mt>,
            $(pub(super) $f: Option<$t>,)*
        }
        impl Drafts {
            /// Number of slots, for the coverage test.
            #[cfg(test)]
            pub(super) const SLOTS: usize = [stringify!($mf) $(, stringify!($f))*].len();
            /// Typed drafts in commit order, with the label used in the refusal message.
            pub(super) const COMMIT: &'static [(&'static str, Apply)] =
                &[$(($label, ProcessCraft::$apply as Apply)),*];
            /// Any slot open, the expression editor included.
            pub(super) fn any(&self) -> bool {
                self.$mf.is_some() $(|| self.$f.is_some())*
            }
            /// Any slot open except the hand-finished expression editor.
            pub(super) fn any_typed(&self) -> bool {
                false $(|| self.$f.is_some())*
            }
            /// An open draft whose panel replaces the ordinary property editors.
            pub(super) fn locks_ordinary_properties(&self) -> bool {
                false $(|| ($locks && self.$f.is_some()))*
            }
            /// Close every slot without applying.
            pub(super) fn clear(&mut self) {
                self.$mf = None;
                $(self.$f = None;)*
            }
        }
    };
}

drafts! {
    manual { expression_draft: super::inspector::ExpressionDraft }
    typed {
        binding_draft: super::inspector::BindingDraft => "activity reference", apply_binding_draft, locks: false;
        xml_id_draft: super::inspector::XmlIdDraft => "XML ID", apply_xml_id_draft, locks: true;
        catalog_draft: super::catalog::CatalogDraft => "supporting catalog", apply_catalog_draft, locks: true;
        data_draft: super::data::DataDraft => "data", apply_data_draft, locks: true;
        graph_data_draft: super::graph_data::GraphDataDraft => "choreography data association", apply_graph_data_draft, locks: true;
        di_draft: super::di::DiDraft => "diagram properties", apply_di_draft, locks: true;
        documentation_draft: super::documentation::DocumentationDraft => "documentation", apply_documentation_draft, locks: true;
        body_draft: super::mixed_body::BodyDraft => "content", apply_body_draft, locks: true;
        metadata_draft: super::base_metadata::MetadataDraft => "advanced metadata", apply_metadata_draft, locks: true;
        style_draft: super::di_style::StyleDraft => "diagram font", apply_style_draft, locks: true;
        definitions_extra_draft: super::definitions_extra::DefinitionsExtraDraft => "definitions details", apply_definitions_extra_draft, locks: true;
        correlation_catalog_draft: super::correlation::CorrelationCatalogDraft => "correlation", apply_correlation_catalog_draft, locks: true;
        activity_roles_draft: super::roles::ActivityRolesDraft => "activity roles", apply_activity_roles_draft, locks: true;
        callable_catalog_draft: super::callable::CallableCatalogDraft => "global task", apply_callable_catalog_draft, locks: true;
        call_draft: super::call::CallDraft => "called element", apply_call_draft, locks: true;
        process_activity_draft: super::process_activity::ProcessActivityDraft => "process behavior", apply_process_activity_draft, locks: true;
        flow_metadata_draft: super::flow_metadata::FlowDraft => "flow details", apply_flow_metadata_draft, locks: true;
        correlation_keys_draft: super::correlation_keys::CorrelationKeysDraft => "correlation keys", apply_correlation_keys_draft, locks: true;
        loop_advanced_draft: super::loop_advanced::LoopAdvancedDraft => "multi-instance", apply_loop_advanced_draft, locks: true;
        choreography_flow_draft: super::choreography_flow::ChoreographyFlowDraft => "choreography details", apply_choreography_flow_draft, locks: true;
        multiplicity_draft: super::participant_di::MultiplicityDraft => "multiplicity", apply_multiplicity_draft, locks: true;
        message_ref_draft: super::participant_di::MessageRefDraft => "message reference", apply_message_ref_draft, locks: true;
        band_draft: super::band_create::BandDraft => "participant band", apply_band_draft, locks: true;
        event_draft: super::event::EventDraft => "event", apply_event_draft, locks: true;
        event_catalog_draft: super::event::EventCatalogDraft => "event catalog", apply_event_catalog_draft, locks: true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The draft types have no `Default`, so the slots cannot all be filled here. The table and the
    // queries come from the same macro list as the struct, so this guards the pieces that are not
    // generated: the commit table covers every slot but the hand-finished expression editor.
    #[test]
    fn commit_table_covers_every_slot_but_the_expression_editor() {
        assert_eq!(Drafts::COMMIT.len() + 1, Drafts::SLOTS);
        let mut drafts = Drafts::default();
        assert!(!drafts.any() && !drafts.any_typed() && !drafts.locks_ordinary_properties());
        drafts.clear();
        assert!(!drafts.any());
    }
}
