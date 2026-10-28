// No console window behind the app on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod code_view;
mod curl;
mod http;
mod json_view;
mod model;
mod theme;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Rustman")
            .with_inner_size([1200.0, 820.0])
            .with_min_inner_size([700.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Rustman",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
