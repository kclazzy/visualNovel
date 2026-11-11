//! Игра: показывает историю из `game_data/story.vnpak` рядом с исполняемым файлом.
//! Редактор для запуска не нужен. Обновления истории (`*.vnpak` в папке игры)
//! устанавливаются при старте, сохранения при этом не затрагиваются.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vn_store::export::{apply_updates, UpdateOutcome, PACK_FILE};
use vn_store::pack::Pack;
use vn_store::saves::SaveStore;
use vn_view::{GameView, PackAssets};

struct Loaded {
    game: GameView,
    notes: Vec<String>,
    resolution: [u32; 2],
    title: String,
}

fn game_dir() -> PathBuf {
    std::env::args_os().nth(1).map(PathBuf::from).or_else(|| std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))).unwrap_or_else(|| PathBuf::from("."))
}

fn load(dir: &Path) -> Result<Loaded, String> {
    let notes = apply_updates(dir, &SaveStore::for_story)
        .into_iter()
        .map(|o| match o {
            UpdateOutcome::Applied(m) => format!("История обновлена до версии {} (выпуск {}). Сохранения на месте, их копия сделана перед обновлением.", m.version, m.story_version),
            UpdateOutcome::Rejected { file, reason } => format!("Обновление «{file}» не установлено: {reason}"),
        })
        .collect();
    let bytes = std::fs::read(dir.join(PACK_FILE)).map_err(|e| format!("Не найден файл истории {PACK_FILE}: {e}"))?;
    let pack = Pack::open(bytes)?;
    let story = pack.story.clone();
    let saves = SaveStore::for_story(&story.meta.id);
    let first_run = !saves.dir().join("settings.cfg").exists();
    let mut game = GameView::new(story.clone(), Arc::new(PackAssets(pack)), saves);
    if first_run {
        let config: serde_json::Value = std::fs::read_to_string(dir.join("config/game.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        game.settings.fullscreen = config["fullscreen"].as_bool().unwrap_or(story.meta.fullscreen);
    }
    Ok(Loaded { game, notes, resolution: story.meta.resolution, title: story.meta.name })
}

struct App {
    game: GameView,
    notes: Vec<String>,
    fullscreen: bool,
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
            self.game.settings.fullscreen = !self.game.settings.fullscreen;
        }
        if self.fullscreen != self.game.settings.fullscreen {
            self.fullscreen = self.game.settings.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| self.game.ui(ui));
        if !self.notes.is_empty() {
            egui::Window::new("Обновление истории").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(&ctx, |ui| {
                for n in &self.notes {
                    ui.label(n);
                }
                if ui.button("Понятно").clicked() {
                    self.notes.clear();
                }
            });
        }
        if self.game.wants_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

struct Failure(String);

impl eframe::App for Failure {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Игра не может запуститься");
            ui.label(&self.0);
        });
    }
}

fn main() -> eframe::Result {
    let dir = game_dir();
    let loaded = load(&dir);
    let (title, size) = match &loaded {
        Ok(l) => (l.title.clone(), [l.resolution[0] as f32, l.resolution[1] as f32]),
        Err(e) => {
            vn_store::fsutil::log_error(&dir, e);
            ("Ошибка".to_string(), [640.0, 240.0])
        }
    };
    let options = eframe::NativeOptions { viewport: egui::ViewportBuilder::default().with_title(&title).with_inner_size(size).with_min_inner_size([480.0, 270.0]), ..Default::default() };
    eframe::run_native(
        &title,
        options,
        Box::new(move |cc| {
            vn_view::install_fonts(&cc.egui_ctx);
            Ok(match loaded {
                Ok(l) => Box::new(App { game: l.game, notes: l.notes, fullscreen: false }) as Box<dyn eframe::App>,
                Err(e) => Box::new(Failure(e)),
            })
        }),
    )
}
