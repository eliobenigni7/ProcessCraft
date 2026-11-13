use egui::{
    Color32, ColorImage, Id, Response, RichText, Sense, TextureHandle, TextureOptions, Ui, Vec2,
    vec2,
};
use std::io::Cursor;

#[derive(Clone, Copy, Debug, Hash)]
pub enum Icon {
    Workflow,
    Folder,
    Book,
    Table,
    Save,
    Undo,
    Redo,
    Plus,
    X,
    Settings,
    PanelLeft,
    PanelRight,
    HardDrive,
    ZoomIn,
    ZoomOut,
    Maximize,
    Alert,
    Comment,
    History,
    Check,
    Focus,
    Download,
}

impl Icon {
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::PanelRight => include_bytes!("../../../assets/ui-icons/panel-right.png"),
            Self::Undo => include_bytes!("../../../assets/ui-icons/undo-2.png"),
            Self::Plus => include_bytes!("../../../assets/ui-icons/plus.png"),
            Self::ZoomIn => include_bytes!("../../../assets/ui-icons/zoom-in.png"),
            Self::Redo => include_bytes!("../../../assets/ui-icons/redo-2.png"),
            Self::Book => include_bytes!("../../../assets/ui-icons/book-open.png"),
            Self::Folder => include_bytes!("../../../assets/ui-icons/folder.png"),
            Self::Settings => include_bytes!("../../../assets/ui-icons/settings.png"),
            Self::HardDrive => include_bytes!("../../../assets/ui-icons/hard-drive.png"),
            Self::Table => include_bytes!("../../../assets/ui-icons/list.png"),
            Self::PanelLeft => include_bytes!("../../../assets/ui-icons/panel-left.png"),
            Self::X => include_bytes!("../../../assets/ui-icons/x.png"),
            Self::Save => include_bytes!("../../../assets/ui-icons/save.png"),
            Self::Workflow => include_bytes!("../../../assets/ui-icons/workflow.png"),
            Self::ZoomOut => include_bytes!("../../../assets/ui-icons/zoom-out.png"),
            Self::Maximize => include_bytes!("../../../assets/ui-icons/maximize.png"),
            Self::Alert => include_bytes!("../../../assets/ui-icons/triangle-alert.png"),
            Self::Comment => include_bytes!("../../../assets/ui-icons/message-square.png"),
            Self::History => include_bytes!("../../../assets/ui-icons/history.png"),
            Self::Check => include_bytes!("../../../assets/ui-icons/circle-check.png"),
            Self::Focus => include_bytes!("../../../assets/ui-icons/scan.png"),
            Self::Download => include_bytes!("../../../assets/ui-icons/download.png"),
        }
    }
}

fn texture(ui: &Ui, icon: Icon) -> TextureHandle {
    let id = Id::unique(("processcraft-icon", icon));
    if let Some(handle) = ui.ctx().data(|data| data.get_temp::<TextureHandle>(id)) {
        return handle;
    }
    let decoder = png::Decoder::new(Cursor::new(icon.bytes()));
    let mut reader = decoder.read_info().expect("embedded icon PNG");
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut pixels)
        .expect("embedded icon pixels");
    let image = ColorImage::from_rgba_unmultiplied(
        [info.width as usize, info.height as usize],
        &pixels[..info.buffer_size()],
    );
    let handle = ui.ctx().load_texture(
        format!("processcraft-{icon:?}"),
        image,
        TextureOptions::LINEAR,
    );
    ui.ctx()
        .data_mut(|data| data.insert_temp(id, handle.clone()));
    handle
}

pub fn paint_icon(ui: &Ui, icon: Icon, rect: egui::Rect, color: Color32) {
    let texture = texture(ui, icon);
    ui.painter().image(
        texture.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        color,
    );
}

pub fn icon(ui: &mut Ui, icon: Icon, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_icon(ui, icon, rect, color);
    }
    response
}

pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str, selected: bool) -> Response {
    icon_button_sized(ui, icon, label, selected, 32.0)
}

pub fn icon_button_sized(
    ui: &mut Ui,
    icon: Icon,
    label: &str,
    selected: bool,
    size: f32,
) -> Response {
    let style = icon_button_style(ui);
    craft_ui::buttons::IconButton::new(label, Vec2::splat(size), &style)
        .selected(selected)
        .show(ui, |ui, rect, color| {
            paint_icon(
                ui,
                icon,
                egui::Rect::from_center_size(
                    rect.center(),
                    Vec2::splat((size - 14.0).clamp(12.0, 18.0)),
                ),
                color,
            )
        })
        .on_hover_text(label)
}

/// Quiet icon buttons: transparent at rest, subtle fill on hover, amber when selected.
pub fn icon_button_style(ui: &Ui) -> craft_ui::buttons::IconButtonStyle {
    use craft_ui::buttons::{ButtonVisuals, IconButtonStyle};
    let p = crate::theme::current(ui);
    let w = &ui.visuals().widgets;
    let state = |fill, icon| ButtonVisuals {
        fill,
        stroke: egui::Stroke::NONE,
        icon,
    };
    let disabled = !ui.is_enabled();
    IconButtonStyle {
        normal: state(
            Color32::TRANSPARENT,
            if disabled { p.faint } else { p.muted },
        ),
        hovered: state(w.hovered.bg_fill, p.text),
        pressed: Some(state(w.active.bg_fill, p.text)),
        selected: state(p.accent_soft, p.accent_ink),
        corner_radius: crate::theme::RADIUS.into(),
        focus_stroke: egui::Stroke::new(1.5, p.accent),
    }
}

/// The ProcessCraft app icon (assets/app-icon, generated by scripts/generate_app_icon.py).
pub fn application_icon() -> egui::IconData {
    const ICON: &[u8] = include_bytes!("../../../assets/app-icon/processcraft-256.png");
    let mut reader = png::Decoder::new(Cursor::new(ICON))
        .read_info()
        .expect("embedded application icon");
    let mut rgba = vec![0; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut rgba)
        .expect("embedded application icon pixels");
    rgba.truncate(info.buffer_size());
    egui::IconData {
        rgba,
        width: info.width,
        height: info.height,
    }
}

pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    let p = crate::theme::current(ui);
    ui.add(
        egui::Button::new(
            RichText::new(text)
                .font(crate::theme::medium(13.0))
                .color(p.on_accent),
        )
        .fill(p.accent)
        .stroke(egui::Stroke::NONE)
        .min_size(vec2(0.0, crate::theme::INTERACT_HEIGHT)),
    )
}
pub fn section_label(ui: &mut Ui, text: &str) -> Response {
    ui.label(
        RichText::new(text.to_uppercase())
            .font(crate::theme::semibold(11.0))
            .color(crate::theme::current(ui).faint),
    )
}
fn section_id(title: &str) -> Id {
    Id::unique(("processcraft-section", title))
}

/// Opens the named section the next time it is shown (e.g. from a menu command).
pub fn open_section(ctx: &egui::Context, title: &str) {
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ctx,
        section_id(title),
        true,
    );
    state.set_open(true);
    state.store(ctx);
}

/// A collapsible inspector section: a full-width header row (chevron, SemiBold title, optional
/// count) above a hairline, with its open state remembered for the session.
pub fn section<R>(
    ui: &mut Ui,
    title: &str,
    count: Option<usize>,
    default_open: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    let p = crate::theme::current(ui);
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
        ui.ctx(),
        section_id(title),
        default_open,
    );
    ui.add_space(2.0);
    let top = ui.available_rect_before_wrap().top();
    ui.painter().hline(
        ui.max_rect().x_range(),
        top,
        egui::Stroke::new(1.0, p.border),
    );
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, true, state.is_open(), title)
    });
    if response.clicked() {
        state.toggle(ui);
    }
    let openness = state.openness(ui.ctx());
    let chevron = row.left_center() + vec2(6.0, 0.0);
    let rotation = egui::emath::Rot2::from_angle(openness * std::f32::consts::FRAC_PI_2);
    let points =
        [vec2(-2.5, -4.0), vec2(2.5, 0.0), vec2(-2.5, 4.0)].map(|v| chevron + rotation * v);
    ui.painter().add(egui::Shape::line(
        points.to_vec(),
        egui::Stroke::new(1.4, if response.hovered() { p.text } else { p.faint }),
    ));
    ui.painter().text(
        row.left_center() + vec2(18.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        crate::theme::semibold(13.0),
        p.text,
    );
    if let Some(count) = count.filter(|n| *n > 0) {
        let text = count.to_string();
        let pill = egui::Rect::from_min_size(
            egui::pos2(
                row.right() - 8.0 - 7.0 * text.len() as f32 - 8.0,
                row.center().y - 8.0,
            ),
            vec2(7.0 * text.len() as f32 + 8.0, 16.0),
        );
        ui.painter().rect_filled(pill, 8.0, p.secondary);
        ui.painter().text(
            pill.center(),
            egui::Align2::CENTER_CENTER,
            text,
            crate::theme::medium(10.5),
            p.muted,
        );
    }
    state
        .show_body_unindented(ui, |ui| {
            let r = add(ui);
            ui.add_space(8.0);
            r
        })
        .map(|body| body.inner)
}

pub fn badge(ui: &mut Ui, text: &str, foreground: Color32, background: Color32) -> Response {
    egui::Frame::new()
        .fill(background)
        .corner_radius(crate::theme::RADIUS_SMALL)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .font(crate::theme::medium(11.0))
                    .color(foreground),
            )
        })
        .inner
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_application_icon_decodes_to_256_rgba() {
        let icon = super::application_icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }
}
