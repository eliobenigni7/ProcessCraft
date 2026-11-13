//! Design tokens, UI fonts and the egui style (docs/plans/ux-shell.md, "Visual system").
//!
//! Light first; dark derives from the same roles. The accent is the ProcessCraft app colour
//! (amber, assets/app-icon/README.md); blue is reserved for canvas selection.

use craft_ui::fonts::{CraftFace, Family, I18nScripts, SystemFonts, UiFonts};
use egui::{
    Color32, Context, CornerRadius, FontData, FontFamily, FontId, Shadow, Stroke, TextStyle,
    Visuals, vec2,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Clone, Copy)]
pub struct Palette {
    /// Raised surfaces: inspector, popups, inputs' container.
    pub base: Color32,
    /// Chrome surfaces: menu bar, sidebar, status bar.
    pub panel: Color32,
    /// Subtle fill: hover, input fill, lane headers.
    pub secondary: Color32,
    pub canvas: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    /// Amber fill (identity, primary actions, active markers).
    pub accent: Color32,
    /// Text and icons drawn in the accent colour.
    pub accent_ink: Color32,
    /// Text on an accent fill.
    pub on_accent: Color32,
    pub accent_soft: Color32,
    /// Canvas selection and snap guides.
    pub selection: Color32,
    pub selection_soft: Color32,
    pub warning: Color32,
    pub warning_soft: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
    pub success: Color32,
}

pub const RADIUS_SMALL: u8 = 4;
pub const RADIUS: u8 = 6;
pub const RADIUS_CARD: u8 = 8;
pub const RADIUS_FLOAT: u8 = 12;
pub const INTERACT_HEIGHT: f32 = 28.0;

const MEDIUM: &str = "medium";
const SEMIBOLD: &str = "semibold";

pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

/// The palette of the theme currently applied to `ui`.
pub fn current(ui: &egui::Ui) -> Palette {
    if ui.visuals().dark_mode {
        Theme::Dark
    } else {
        Theme::Light
    }
    .palette()
}

impl Theme {
    pub fn palette(self) -> Palette {
        let dark = self == Self::Dark;
        let c = |light: u32, dark_color: u32| {
            let v = if dark { dark_color } else { light };
            Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
        };
        Palette {
            base: c(0xffffff, 0x1e2126),
            panel: c(0xf7f8fa, 0x17191d),
            secondary: c(0xf0f2f5, 0x262a30),
            canvas: c(0xfafbfc, 0x121417),
            text: c(0x16181d, 0xeceef2),
            muted: c(0x5b6270, 0xa8afbc),
            faint: c(0x8a91a0, 0x6f7684),
            border: c(0xe3e6eb, 0x2e333a),
            border_strong: c(0xcdd2da, 0x454b54),
            accent: c(0xf2a516, 0xf2a516),
            accent_ink: c(0x9a6400, 0xf5b940),
            on_accent: c(0x0b0b0c, 0x0b0b0c),
            accent_soft: c(0xfdf3dc, 0x3a2e14),
            selection: c(0x1e88e5, 0x4aa3f0),
            selection_soft: c(0xe6f2fd, 0x1a2e44),
            warning: c(0xe8590c, 0xf79009),
            warning_soft: c(0xfff1e8, 0x3d2414),
            danger: c(0xd92d20, 0xf2635a),
            danger_soft: c(0xfeeceb, 0x3a1d1b),
            success: c(0x12a150, 0x3ccb7f),
        }
    }

    /// Elevation e1 (floating canvas capsules); dark mode uses a border instead.
    pub fn float_shadow(self) -> Shadow {
        if self == Self::Dark {
            Shadow::NONE
        } else {
            Shadow {
                offset: [0, 4],
                blur: 12,
                spread: 0,
                color: Color32::from_black_alpha(22),
            }
        }
    }

    /// The border of floating surfaces: hairline white at 8% in dark mode.
    pub fn float_stroke(self) -> Stroke {
        if self == Self::Dark {
            Stroke::new(1.0, Color32::from_white_alpha(20))
        } else {
            Stroke::new(1.0, self.palette().border)
        }
    }

    pub fn apply(self, context: &Context) {
        let id = egui::Id::unique("processcraft-applied-theme");
        let dark = self == Self::Dark;
        if context.data(|data| data.get_temp::<bool>(id)) == Some(dark) {
            return;
        }
        context.data_mut(|data| data.insert_temp(id, dark));
        install_fonts(context);
        let p = self.palette();
        let egui_theme = if dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        let mut style = (*context.style_of(egui_theme)).clone();
        for (text_style, font) in [
            (TextStyle::Body, regular(13.0)),
            (TextStyle::Button, regular(13.0)),
            (TextStyle::Small, regular(11.0)),
            (TextStyle::Monospace, mono(12.0)),
            (TextStyle::Heading, semibold(16.0)),
        ] {
            style.text_styles.insert(text_style, font);
        }
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.button_padding = vec2(10.0, 5.0);
        style.spacing.interact_size = vec2(36.0, INTERACT_HEIGHT);
        style.spacing.menu_margin = egui::Margin::same(6);
        style.spacing.window_margin = egui::Margin::same(12);
        let mut v = if dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        v.panel_fill = p.panel;
        v.window_fill = p.base;
        v.extreme_bg_color = p.secondary;
        v.faint_bg_color = p.secondary;
        v.code_bg_color = p.secondary;
        v.override_text_color = Some(p.text);
        v.weak_text_color = Some(p.muted);
        v.hyperlink_color = p.accent_ink;
        v.warn_fg_color = p.warning;
        v.error_fg_color = p.danger;
        v.selection.bg_fill = p.accent_soft;
        v.selection.stroke = Stroke::new(1.0, p.accent_ink);
        v.window_corner_radius = CornerRadius::same(RADIUS_CARD);
        v.menu_corner_radius = CornerRadius::same(RADIUS);
        v.window_stroke = self.float_stroke();
        let popup = if dark {
            Shadow {
                offset: [0, 8],
                blur: 24,
                spread: 0,
                color: Color32::from_black_alpha(120),
            }
        } else {
            Shadow {
                offset: [0, 8],
                blur: 24,
                spread: 0,
                color: Color32::from_black_alpha(36),
            }
        };
        v.window_shadow = popup;
        v.popup_shadow = popup;
        let hover = if dark {
            Color32::from_rgb(0x2e, 0x33, 0x3a)
        } else {
            Color32::from_rgb(0xe8, 0xeb, 0xef)
        };
        let pressed = if dark {
            Color32::from_rgb(0x36, 0x3c, 0x44)
        } else {
            Color32::from_rgb(0xdf, 0xe3, 0xe8)
        };
        craft_ui::theme::WidgetPalette {
            noninteractive: p.panel,
            inactive: p.secondary,
            hovered: hover,
            active: pressed,
            open: hover,
        }
        .apply_to(
            &mut v.widgets,
            CornerRadius::same(RADIUS),
            Stroke::new(1.0, p.text),
        );
        // Fill-based controls: no outline at rest, a quiet outline on hover, accent when pressed.
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.muted);
        v.widgets.inactive.bg_stroke = Stroke::NONE;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
        v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
        v.widgets.open.bg_stroke = Stroke::new(1.0, p.border_strong);
        v.widgets.hovered.expansion = 0.0;
        v.widgets.active.expansion = 0.0;
        style.visuals = v;
        context.set_style_of(egui_theme, style);
        context.set_theme(egui_theme);
    }
}

/// Inter for the interface and JetBrains Mono for identifiers, as in the office apps of the
/// Crafting Apps family; the bundled Noto faces cover Arabic and Chinese. Installed system fonts
/// are not used, so text draws the same on every machine.
fn install_fonts(context: &Context) {
    let face = |bytes: &'static [u8]| FontData::from_static(bytes);
    let craft = [
        CraftFace {
            family: "Noto Sans Arabic",
            style: "Regular",
            scripts: &["Arab"],
            bytes: include_bytes!("../../../assets/fonts/NotoSansArabic-Regular.ttf"),
            index: 0,
        },
        CraftFace {
            family: "Noto Sans CJK SC",
            style: "Regular",
            scripts: &["Hans"],
            bytes: include_bytes!("../../../assets/fonts/NotoSansCJKsc-Regular.otf"),
            index: 0,
        },
    ];
    UiFonts::new(&I18nScripts)
        .app_face(
            "Inter",
            face(include_bytes!("../../../assets/fonts/Inter-Regular.ttf")),
            &[FontFamily::Proportional],
        )
        .app_face(
            "Inter Medium",
            face(include_bytes!("../../../assets/fonts/Inter-Medium.ttf")),
            &[FontFamily::Name(MEDIUM.into())],
        )
        .app_face(
            "Inter SemiBold",
            face(include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf")),
            &[FontFamily::Name(SEMIBOLD.into())],
        )
        .app_face(
            "JetBrains Mono",
            face(include_bytes!(
                "../../../assets/fonts/JetBrainsMono-Regular.ttf"
            )),
            &[FontFamily::Monospace],
        )
        .family(Family::named(MEDIUM).weight(500))
        .family(Family::named(SEMIBOLD).weight(600))
        // egui's own faces for symbols Inter lacks (▾ ✓ ⋯ ⏵).
        .egui_face("Hack", &[FontFamily::Proportional])
        .egui_face("NotoEmoji-Regular", &[FontFamily::Proportional])
        .egui_face("emoji-icon-font", &[FontFamily::Proportional])
        .craft_faces(&craft)
        .languages(&["en"])
        .system_fonts(SystemFonts::Off)
        .install(context);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interface_fonts_cover_latin_arabic_and_chinese() {
        let context = Context::default();
        Theme::Light.apply(&context);
        context
            .run_ui(Default::default(), |_| {})
            .textures_delta
            .clear();
        context.fonts_mut(|fonts| {
            for (font, text) in [
                (regular(13.0), "Approve purchase order"),
                (semibold(14.0), "General"),
                (mono(12.0), "Activity_1x9"),
                (regular(13.0), "موافقة"),
                (regular(13.0), "审批"),
                (regular(13.0), "▾✓⋯"),
            ] {
                for ch in text.chars().filter(|c| !c.is_whitespace()) {
                    assert!(fonts.has_glyph(&font, ch), "{ch:?} missing in {font:?}");
                }
            }
        });
    }
}
