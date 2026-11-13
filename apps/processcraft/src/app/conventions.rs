//! "Conventions..." window of the Problems tab (M5.2): Off/Warning/Error for each
//! modeling-convention rule, the element limit and the verb list. Apply submits
//! one `SetConventionsProfile`; results come back through `DiagnosticsWorker`.
use super::*;
use processcraft_core::{
    attributes::AttributeTarget,
    conventions::{ConventionRule, ConventionsProfile},
    validation::DiagnosticSeverity,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Level {
    Off,
    Warning,
    Error,
}
impl Level {
    fn severity(self) -> Option<DiagnosticSeverity> {
        match self {
            Level::Off => None,
            Level::Warning => Some(DiagnosticSeverity::Warning),
            Level::Error => Some(DiagnosticSeverity::Error),
        }
    }
    fn of(severity: DiagnosticSeverity) -> Self {
        match severity {
            DiagnosticSeverity::Warning => Level::Warning,
            DiagnosticSeverity::Error => Level::Error,
        }
    }
}

/// Rule labels in the order of [`Draft::levels`].
const RULES: [&str; 8] = [
    "Label required",
    "Activity label is verb + object (word-list heuristic)",
    "Labels on gateway flows",
    "No implicit split or join",
    "Single start and end event per scope",
    "Maximum shapes per diagram",
    "Required attributes have values",
    "Dictionary terms (roles on lanes, no free text)",
];
const KINDS: [(AttributeTarget, &str); 8] = [
    (AttributeTarget::Activity, "Activities"),
    (AttributeTarget::Event, "Events"),
    (AttributeTarget::Gateway, "Gateways"),
    (AttributeTarget::SequenceFlow, "Sequence flows"),
    (AttributeTarget::Lane, "Lanes"),
    (AttributeTarget::Participant, "Participants"),
    (AttributeTarget::Process, "Processes"),
    (AttributeTarget::DataObject, "Data objects"),
];

/// Editable copy of the profile.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Draft {
    levels: [Level; 8],
    kinds: Vec<AttributeTarget>,
    limit: u32,
    /// Whitespace or comma separated; empty means the built-in list.
    verbs: String,
}
impl Draft {
    fn from_profile(profile: Option<&ConventionsProfile>) -> Self {
        let mut draft = Draft {
            levels: [Level::Off; 8],
            kinds: vec![
                AttributeTarget::Activity,
                AttributeTarget::Event,
                AttributeTarget::Lane,
                AttributeTarget::Participant,
            ],
            limit: 50,
            verbs: String::new(),
        };
        for rule in profile.map_or(&[][..], |p| p.rules.as_slice()) {
            let (index, level) = match rule {
                ConventionRule::LabelRequired { severity, kinds } => {
                    draft.kinds = kinds.clone();
                    (0, severity)
                }
                ConventionRule::ActivityVerbObject { severity, verbs } => {
                    draft.verbs = verbs.join(" ");
                    (1, severity)
                }
                ConventionRule::GatewayFlowsLabelled { severity } => (2, severity),
                ConventionRule::NoImplicitSplitJoin { severity } => (3, severity),
                ConventionRule::SingleStartEnd { severity } => (4, severity),
                ConventionRule::MaxElements { severity, limit } => {
                    draft.limit = *limit;
                    (5, severity)
                }
                ConventionRule::RequiredAttributes { severity } => (6, severity),
                ConventionRule::DictionaryTerms { severity } => (7, severity),
            };
            draft.levels[index] = Level::of(*level);
        }
        draft
    }

    /// `None` when every rule is off; core bounds are checked by `validate`.
    fn to_profile(&self) -> Result<Option<ConventionsProfile>, String> {
        let mut rules = Vec::new();
        for (index, level) in self.levels.iter().enumerate() {
            let Some(severity) = level.severity() else {
                continue;
            };
            rules.push(match index {
                0 => ConventionRule::LabelRequired {
                    severity,
                    kinds: self.kinds.clone(),
                },
                1 => ConventionRule::ActivityVerbObject {
                    severity,
                    verbs: self
                        .verbs
                        .split(|c: char| c.is_whitespace() || c == ',')
                        .filter(|word| !word.is_empty())
                        .map(str::to_owned)
                        .collect(),
                },
                2 => ConventionRule::GatewayFlowsLabelled { severity },
                3 => ConventionRule::NoImplicitSplitJoin { severity },
                4 => ConventionRule::SingleStartEnd { severity },
                5 => ConventionRule::MaxElements {
                    severity,
                    limit: self.limit,
                },
                6 => ConventionRule::RequiredAttributes { severity },
                _ => ConventionRule::DictionaryTerms { severity },
            });
        }
        if rules.is_empty() {
            return Ok(None);
        }
        let profile = ConventionsProfile { rules };
        profile.validate().map_err(|error| match error {
            processcraft_core::commands::CommandError::InvalidParams(message) => message,
            other => format!("{other:?}"),
        })?;
        Ok(Some(profile))
    }
}

#[derive(Default)]
pub(super) struct State {
    pub(super) draft: Option<Draft>,
}

impl ProcessCraft {
    pub(super) fn open_conventions(&mut self) {
        let snapshot = self.ctl.session.snapshot();
        self.conventions.draft = Some(Draft::from_profile(snapshot.project.conventions.as_ref()));
    }

    /// Submits the draft as one `SetConventionsProfile`; the window stays open on a bound error.
    pub(super) fn apply_conventions(&mut self) -> bool {
        let Some(draft) = &self.conventions.draft else {
            return true;
        };
        match draft.to_profile() {
            Ok(profile) => self
                .ctl
                .mutate(vec![Mutation::SetConventionsProfile { profile }]),
            Err(message) => {
                self.ctl.error = message;
                false
            }
        }
    }

    pub(super) fn conventions_window(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.conventions.draft.clone() else {
            return;
        };
        let mut open = true;
        let (mut apply, mut cancel) = (false, false);
        let problem = draft.to_profile().err();
        egui::Window::new("Modeling conventions")
            .open(&mut open)
            .collapsible(false)
            .default_width(460.0)
            .max_width((ctx.content_rect().width() - 32.0).max(150.0))
            .vscroll(true)
            .show(ctx, |ui| {
                ui.label("Rules that are off are not checked. Results appear under Problems.");
                for (index, label) in RULES.iter().enumerate() {
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt(("convention-level", index))
                            .selected_text(match draft.levels[index] {
                                Level::Off => "Off",
                                Level::Warning => "Warning",
                                Level::Error => "Error",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut draft.levels[index], Level::Off, "Off");
                                ui.selectable_value(
                                    &mut draft.levels[index],
                                    Level::Warning,
                                    "Warning",
                                );
                                ui.selectable_value(
                                    &mut draft.levels[index],
                                    Level::Error,
                                    "Error",
                                );
                            });
                        ui.label(*label);
                    });
                    match index {
                        0 => ui.horizontal_wrapped(|ui| {
                            for (kind, name) in KINDS {
                                let mut on = draft.kinds.contains(&kind);
                                if ui.checkbox(&mut on, name).changed() {
                                    draft.kinds.retain(|k| *k != kind);
                                    if on {
                                        draft.kinds.push(kind);
                                    }
                                }
                            }
                        }),
                        1 => ui.horizontal(|ui| {
                            ui.label("Verbs (empty: built-in English list)");
                            ui.add(
                                egui::TextEdit::multiline(&mut draft.verbs)
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY),
                            );
                        }),
                        5 => ui.horizontal(|ui| {
                            ui.label("Limit");
                            ui.add(egui::DragValue::new(&mut draft.limit).range(1..=10_000));
                        }),
                        _ => ui.horizontal(|_| {}),
                    };
                }
                if let Some(problem) = &problem {
                    ui.colored_label(self.theme.palette().danger, problem);
                }
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(problem.is_none(), |ui| {
                        apply = ui.button("Apply").clicked();
                    });
                    cancel = ui.button("Cancel").clicked();
                });
            });
        self.conventions.draft = Some(draft);
        if apply && self.apply_conventions() {
            self.conventions.draft = None;
        }
        if cancel || !open {
            self.conventions.draft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_round_trips_and_apply_is_one_undo_step() {
        let mut app = ProcessCraft::default();
        app.open_conventions();
        // All rules off submits nothing wrong: the stored profile stays absent.
        assert_eq!(
            app.conventions.draft.as_ref().unwrap().to_profile(),
            Ok(None)
        );
        let recommended = ConventionsProfile::recommended();
        let draft = Draft::from_profile(Some(&recommended));
        assert_eq!(draft.to_profile(), Ok(Some(recommended.clone())));

        let before = app.ctl.session.snapshot().generation;
        app.conventions.draft = Some(draft);
        assert!(app.apply_conventions());
        let snapshot = app.ctl.session.snapshot();
        assert_eq!(snapshot.generation, before + 1);
        assert_eq!(snapshot.project.conventions, Some(recommended));
        assert!(app.ctl.apply(Request::Undo));
        assert!(app.ctl.session.snapshot().project.conventions.is_none());
    }

    #[test]
    fn out_of_bounds_draft_is_not_submitted() {
        let mut app = ProcessCraft::default();
        let mut draft = Draft::from_profile(None);
        draft.levels[5] = Level::Error;
        draft.limit = 0;
        app.conventions.draft = Some(draft);
        let before = app.ctl.session.snapshot().generation;
        assert!(!app.apply_conventions());
        assert_eq!(app.ctl.session.snapshot().generation, before);
        assert!(app.ctl.error.contains("limit"), "{}", app.ctl.error);
    }
}
