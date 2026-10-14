#![allow(dead_code)]

mod ao3;
mod app;
mod reader;
mod storage;
mod ui;

use app::AO3ReaderApp;

#[tokio::main]
async fn main() -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 800.0])
            .with_min_inner_size([680.0, 520.0])
            .with_title("AO3 Reader — Archive of Our Own Desktop E-Reader"),
        ..Default::default()
    };

    eframe::run_native(
        "AO3 Desktop Reader",
        native_options,
        Box::new(|cc| Ok(Box::new(AO3ReaderApp::new(cc)))),
    )
}
