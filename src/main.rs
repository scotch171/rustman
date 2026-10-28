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
            .with_min_inner_size([460.0, 380.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
                    .expect("bundled icon is a valid PNG"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "Rustman",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
