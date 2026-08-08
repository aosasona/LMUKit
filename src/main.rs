#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod input;
mod storage;

use app::LmuKitApp;

fn main() -> eframe::Result<()> {
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/lmukit-256.png"))
        .expect("embedded LMUKit icon should be a valid PNG");
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([620.0, 780.0])
            .with_min_inner_size([520.0, 680.0])
            .with_max_inner_size([760.0, 1000.0])
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "LMUKit",
        options,
        Box::new(|cc| Ok(Box::new(LmuKitApp::new(cc)))),
    )
}
