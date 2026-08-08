#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod input;
mod storage;

use app::WillaApp;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([620.0, 780.0])
            .with_min_inner_size([520.0, 680.0])
            .with_max_inner_size([760.0, 1000.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Willa",
        options,
        Box::new(|cc| Ok(Box::new(WillaApp::new(cc)))),
    )
}
