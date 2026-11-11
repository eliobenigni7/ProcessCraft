use egui::{
    Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals,
    vec2,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub base: Color32,
    pub panel: Color32,
    pub secondary: Color32,
    pub canvas: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub warning: Color32,
    pub warning_soft: Color32,
}

impl Theme {
    pub fn palette(self) -> Palette {
        let dark = self == Self::Dark;
        let c = |light: u32, dark_color: u32| {
            let v = if dark { dark_color } else { light };
            Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
        };
        Palette {
            base: c(0xffffff, 0x19232e),
            panel: c(0xf7f9fb, 0x202d3b),
            secondary: c(0xedf1f5, 0x2c3b4c),
            canvas: c(0xf2f5f8, 0x14202b),
            text: c(0x263442, 0xeaf0f7),
            muted: c(0x5c6d81, 0xafbdd0),
            border: c(0xdce3eb, 0x3b4a5b),
            border_strong: c(0xb8c4d1, 0x566b82),
            accent: c(0x2463d4, 0x7aa9ff),
            accent_soft: c(0xeaf1ff, 0x263f62),
            warning: c(0x956118, 0xe6bc70),
            warning_soft: c(0xfff5df, 0x433820),
        }
    }
    pub fn apply(self, context: &Context) {
        let id = egui::Id::unique("processcraft-applied-theme");
        let dark = self == Self::Dark;
        if context.data(|data| data.get_temp::<bool>(id)) == Some(dark) {
            return;
        }
        context.data_mut(|data| data.insert_temp(id, dark));
        let p = self.palette();
        let mut fonts = FontDefinitions::default();
        fonts.font_data.insert(
            "noto".into(),
            FontData::from_static(include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf"))
                .into(),
        );
        fonts.font_data.insert(
            "noto-arabic".into(),
            FontData::from_static(include_bytes!(
                "../../../assets/fonts/NotoSansArabic-Regular.ttf"
            ))
            .into(),
        );
        fonts.font_data.insert(
            "noto-cjk".into(),
            FontData::from_static(include_bytes!(
                "../../../assets/fonts/NotoSansCJKsc-Regular.otf"
            ))
            .into(),
        );
        let family = fonts.families.entry(FontFamily::Proportional).or_default();
        family.insert(0, "noto".into());
        family.push("noto-arabic".into());
        family.push("noto-cjk".into());
        context.set_fonts(fonts);
        let egui_theme = if self == Self::Light {
            egui::Theme::Light
        } else {
            egui::Theme::Dark
        };
        let mut style = (*context.style_of(egui_theme)).clone();
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(24.0));
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(12.0, 7.0);
        style.spacing.interact_size = vec2(40.0, 32.0);
        let mut v = if self == Self::Light {
            Visuals::light()
        } else {
            Visuals::dark()
        };
        v.panel_fill = p.panel;
        v.window_fill = p.base;
        v.extreme_bg_color = p.base;
        v.faint_bg_color = p.secondary;
        v.override_text_color = Some(p.text);
        v.selection.bg_fill = p.accent_soft;
        v.selection.stroke = Stroke::new(1.0, p.accent);
        v.widgets.noninteractive.bg_fill = p.panel;
        v.widgets.noninteractive.weak_bg_fill = p.panel;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
        v.widgets.inactive.bg_fill = p.base;
        v.widgets.inactive.weak_bg_fill = p.base;
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.border);
        v.widgets.hovered.bg_fill = p.secondary;
        v.widgets.hovered.weak_bg_fill = p.secondary;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, p.border_strong);
        v.widgets.active.bg_fill = p.accent_soft;
        v.widgets.active.weak_bg_fill = p.accent_soft;
        v.widgets.active.bg_stroke = Stroke::new(1.0, p.accent);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = 4.into();
        }
        style.visuals = v;
        context.set_style_of(egui_theme, style);
        context.set_theme(egui_theme);
    }
}
