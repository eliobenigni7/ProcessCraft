#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
#[cfg(windows)]
mod attachment;
mod controller;
mod effects;
mod jobs;
mod theme;
mod widgets;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_icon(widgets::application_icon())
            .with_min_inner_size([640.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "ProcessCraft",
        options,
        Box::new(|cc| {
            let app = app::ProcessCraft::default();
            app.apply_theme(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}
