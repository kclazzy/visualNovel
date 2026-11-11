//! Visual Novel Studio — редактор визуальных новелл.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use vn_studio::{config, update, Studio};

struct App(Studio);

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.0.ui(ui));
    }
}

fn main() -> eframe::Result {
    if let Ok(dir) = update::program_dir() {
        update::cleanup(&dir);
    }
    let mut studio = Studio::new(config::default_path());
    if let Some(dir) = std::env::args_os().nth(1) {
        studio.open_project(std::path::Path::new(&dir));
    }
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_title("Visual Novel Studio").with_inner_size([1440.0, 860.0]).with_min_inner_size([1000.0, 600.0]).with_drag_and_drop(true), ..Default::default() };
    eframe::run_native(
        "Visual Novel Studio",
        options,
        Box::new(|cc| {
            vn_view::install_fonts(&cc.egui_ctx);
            cc.egui_ctx.set_theme(egui::Theme::Dark);
            Ok(Box::new(App(studio)))
        }),
    )
}
