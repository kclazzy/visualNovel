//! Главное окно редактора: проект, панели, меню, диалоги.
use crate::ai_panel::AiPanel;
use crate::config::{self, AppConfig};
use crate::history::History;
use crate::update::{self, Check};
use crate::{extra, graph, left, ops, props, stage, timeline, widgets};
use egui::{Color32, Key, Modifiers, RichText, Ui};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use vn_core::model::*;
use vn_core::validate::{has_errors, validate, Issue};
use vn_store::project::{self, AssetKind};
use vn_store::saves::SaveStore;
use vn_view::{DirAssets, GameView, Textures};

pub const ACCENT: Color32 = Color32::from_rgb(108, 99, 255);
const AUTOSAVE_SECONDS: f64 = 120.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Stage,
    Graph,
    Preview,
    Wardrobe,
    Endings,
    Ai,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeftTab {
    Scenes,
    Characters,
    Assets,
    Variables,
    Project,
}

pub enum Confirm {
    Scene { id: SceneId, redirect: Option<SceneId> },
    Asset(String),
    Character(CharId),
    Chapter(ChapterId),
    Autosave(Box<Story>),
}

/// Перетаскиваемое из левой панели на сцену.
#[derive(Clone)]
pub enum Dragged {
    Asset(String),
    Character(CharId),
}

pub struct Built {
    pub message: String,
    pub path: PathBuf,
    pub web: bool,
}

pub struct Project {
    pub dir: PathBuf,
    pub story: Story,
    pub history: History<Story>,
    saved: Story,
    pub files: BTreeSet<String>,
    pub scene: Option<SceneId>,
    pub event: Option<EventId>,
    pub tab: Tab,
    pub left: LeftTab,
    pub textures: Textures,
    pub game: Option<GameView>,
    pub issues: Option<Vec<Issue>>,
    pub graph_view: egui::Rect,
    pub linking: Option<SceneId>,
    pub search: String,
    pub character: Option<CharId>,
    pub confirm: Option<Confirm>,
    pub audio: vn_view::audio::Audio,
    pub try_on: BTreeMap<SlotId, ItemId>,
    pub status: String,
    pub notes: Vec<String>,
    pub built: Option<Built>,
    pub lan: Option<crate::lanserve::Server>,
    pub release: Option<(String, bool)>,
    /// Раздел, в который попадают перетащенные в окно изображения.
    pub import_kind: AssetKind,
    changed_at: Option<f64>,
    backed_up: bool,
}

impl Project {
    pub fn open(dir: &Path) -> Result<Project, String> {
        let loaded = project::load(dir)?;
        let mut story = loaded.story;
        ops::auto_layout(&mut story, false);
        // Примерка в редакторе начинается с первого доступного предмета каждого слота.
        let mut story_try_on = BTreeMap::new();
        for item in story.wardrobe.items.iter().filter(|i| i.unlocked) {
            story_try_on.entry(item.slot.clone()).or_insert_with(|| item.id.clone());
        }
        let confirm = project::read_autosave(dir).filter(|s| *s != story).map(|s| Confirm::Autosave(Box::new(s)));
        Ok(Project {
            dir: dir.to_path_buf(),
            scene: story.meta.start_scene.clone().or_else(|| story.scenes.first().map(|s| s.id.clone())),
            event: None,
            history: History::new(story.clone()),
            saved: story.clone(),
            story,
            files: project::list_assets(dir),
            tab: Tab::Stage,
            left: LeftTab::Scenes,
            textures: Textures::new(Arc::new(DirAssets(dir.to_path_buf()))),
            game: None,
            issues: None,
            graph_view: egui::Rect::ZERO,
            linking: None,
            search: String::new(),
            character: None,
            confirm,
            audio: Default::default(),
            try_on: story_try_on,
            status: String::new(),
            notes: loaded.problems,
            built: None,
            lan: None,
            release: None,
            import_kind: AssetKind::Background,
            changed_at: None,
            backed_up: false,
        })
    }

    pub fn dirty(&self) -> bool {
        self.story != self.saved
    }

    pub fn save(&mut self) -> Result<(), String> {
        if !self.backed_up {
            // Копия состояния, с которого начался сеанс работы: к ней можно вернуться.
            project::backup(&self.dir)?;
            self.backed_up = true;
        }
        project::save(&self.dir, &self.story)?;
        self.saved = self.story.clone();
        self.changed_at = None;
        self.status = "Проект сохранён".into();
        Ok(())
    }

    pub fn rescan(&mut self) {
        self.files = project::list_assets(&self.dir);
        self.textures.forget(None);
    }

    pub fn select(&mut self, scene: Option<SceneId>, event: Option<EventId>) {
        self.scene = scene;
        self.event = event;
    }

    pub fn current_scene(&self) -> Option<&Scene> {
        self.scene.as_ref().and_then(|id| self.story.scene(id))
    }

    /// Индекс выбранного события в текущей сцене.
    pub fn event_index(&self) -> Option<usize> {
        let scene = self.current_scene()?;
        self.event.as_ref().and_then(|e| scene.event_index(e))
    }

    pub fn check(&mut self) -> bool {
        let issues = validate(&self.story, &self.files);
        let ok = !has_errors(&issues);
        self.status = if issues.is_empty() { "Проверка пройдена: ошибок нет".into() } else { format!("Найдено замечаний: {}", issues.len()) };
        self.issues = Some(issues);
        ok
    }

    pub fn start_preview(&mut self, from_scene: bool) {
        let mut game = GameView::new(self.story.clone(), Arc::new(DirAssets(self.dir.clone())), SaveStore::at(self.dir.join("saves")));
        game.can_quit = false;
        game.start(if from_scene { self.scene.clone() } else { None });
        self.audio.stop_all();
        self.game = Some(game);
        self.tab = Tab::Preview;
    }

    /// Скопировать файлы в проект. Возвращает пути добавленных ресурсов.
    pub fn import(&mut self, kind: AssetKind, paths: &[PathBuf]) -> Vec<String> {
        let mut added = vec![];
        let mut errors = vec![];
        for path in paths {
            match project::guess_kind(path, kind).ok_or_else(|| format!("«{}»: этот тип файлов не поддерживается", path.display())).and_then(|k| project::import_asset(&self.dir, k, path)) {
                Ok(asset) => added.push(asset),
                Err(e) => errors.push(e),
            }
        }
        self.files = project::list_assets(&self.dir);
        self.status = if errors.is_empty() { format!("Добавлено файлов: {}", added.len()) } else { errors.join("; ") };
        added
    }

    /// Выбранное не должно указывать на удалённое (после отмены, удаления, ИИ).
    fn fix_selection(&mut self) {
        if self.scene.as_ref().is_some_and(|s| self.story.scene(s).is_none()) || self.scene.is_none() {
            self.scene = self.story.scenes.first().map(|s| s.id.clone());
        }
        if self.event.is_some() && self.event_index().is_none() {
            self.event = None;
        }
        if self.character.as_ref().is_some_and(|c| self.story.character(c).is_none()) {
            self.character = None;
        }
    }

    fn undo(&mut self) {
        if let Some(s) = self.history.undo() {
            self.story = s;
            self.status = "Отменено".into();
        }
    }

    fn redo(&mut self) {
        if let Some(s) = self.history.redo() {
            self.story = s;
            self.status = "Повторено".into();
        }
    }

    fn after_frame(&mut self, now: f64) {
        self.fix_selection();
        if self.history.observe(&self.story, now) {
            self.changed_at.get_or_insert(now);
            self.issues = self.issues.take().map(|_| validate(&self.story, &self.files));
        }
        if self.changed_at.is_some_and(|t| now - t > AUTOSAVE_SECONDS) && self.dirty() {
            self.changed_at = Some(now);
            match project::write_autosave(&self.dir, &self.story) {
                Ok(()) => self.status = "Автосохранение выполнено".into(),
                Err(e) => self.status = format!("Автосохранение не удалось: {e}"),
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NewKind {
    Manual,
    Ai,
    Demo,
}

pub struct NewDialog {
    pub name: String,
    pub folder: String,
    pub kind: NewKind,
}

enum Updater {
    Idle,
    Checking(Receiver<Result<Check, String>>),
    Shown(Result<Check, String>),
    Installing(Receiver<Result<(), String>>),
    Installed,
}

pub struct Studio {
    pub cfg: AppConfig,
    cfg_path: PathBuf,
    pub project: Option<Project>,
    pub new_dialog: Option<NewDialog>,
    pub error: Option<String>,
    pub ai: AiPanel,
    updater: Updater,
    pub wants_restart: bool,
}

enum Build {
    Desktop,
    Web,
}

impl Studio {
    pub fn new(cfg_path: PathBuf) -> Self {
        Self { cfg: AppConfig::load(&cfg_path), cfg_path, project: None, new_dialog: None, error: None, ai: AiPanel::default(), updater: Updater::Idle, wants_restart: false }
    }

    pub fn save_config(&self) {
        self.cfg.save(&self.cfg_path);
    }

    pub fn open_project(&mut self, dir: &Path) {
        match Project::open(dir) {
            Ok(p) => {
                self.cfg.remember(dir);
                self.save_config();
                self.project = Some(p);
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn create_project(&mut self, dir: &Path, name: &str, kind: NewKind) {
        let made = if kind == NewKind::Demo { vn_store::demo::create(dir) } else { project::create(dir, name) };
        match made {
            Ok(_) => {
                self.open_project(dir);
                if let (NewKind::Ai, Some(p)) = (kind, &mut self.project) {
                    p.tab = Tab::Ai;
                }
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn save(&mut self) {
        if let Some(p) = &mut self.project {
            if let Err(e) = p.save() {
                self.error = Some(e);
            }
        }
    }

    fn build(&mut self, what: Build) {
        let Some(p) = &mut self.project else { return };
        if !p.check() {
            p.status = "Сборка остановлена: исправьте ошибки (красные строки в списке замечаний)".into();
            return;
        }
        if let Err(e) = p.save() {
            self.error = Some(e);
            return;
        }
        let result = match what {
            Build::Desktop => update::program_dir().and_then(|d| vn_store::export::export_desktop(&p.dir, &p.story, &d.join(if cfg!(windows) { "vn-player.exe" } else { "vn-player" }))).map(|path| Built { message: "Игра собрана. Эту папку можно копировать на любой компьютер — редактор для запуска не нужен.".into(), path, web: false }),
            Build::Web => vn_store::export::export_web(&p.dir, &p.story).map(|path| Built { message: "Версия для телефона собрана: папка и ZIP-архив рядом с ней. Архив загружается на itch.io или любой хостинг; игра открывается в браузере телефона и ставится на главный экран.".into(), path, web: true }),
        };
        match result {
            Ok(b) => {
                p.lan = None;
                p.built = Some(b);
            }
            Err(e) => self.error = Some(e),
        }
    }

    fn release_update(&mut self, version: &str, with_assets: bool) {
        let Some(p) = &mut self.project else { return };
        if !p.check() {
            p.status = "Выпуск остановлен: исправьте ошибки".into();
            return;
        }
        ops::bump_release(&mut p.story, version);
        let result = p.save().and_then(|()| vn_store::export::export_update(&p.dir, &p.story, with_assets));
        match result {
            Ok(path) => p.built = Some(Built { message: format!("Обновление выпущено (выпуск {}). Игроку достаточно положить этот файл в папку игры и запустить её — сохранения останутся.", p.story.meta.story_version), path, web: false }),
            Err(e) => self.error = Some(e),
        }
    }

    fn check_updates(&mut self) {
        let (tx, rx) = channel();
        std::thread::spawn(move || drop(tx.send(update::check())));
        self.updater = Updater::Checking(rx);
    }

    fn hotkeys(&mut self, ui: &mut Ui) {
        let hit = |ui: &mut Ui, m: Modifiers, k: Key| ui.input_mut(|i| i.consume_key(m, k));
        if hit(ui, Modifiers::COMMAND, Key::S) {
            self.save();
        }
        let typing = ui.memory(|m| m.focused().is_some());
        let Some(p) = &mut self.project else { return };
        if hit(ui, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || hit(ui, Modifiers::COMMAND, Key::Y) {
            p.redo();
        } else if !typing && hit(ui, Modifiers::COMMAND, Key::Z) {
            p.undo();
        }
        if hit(ui, Modifiers::NONE, Key::F5) {
            p.start_preview(false);
        }
        if !typing && p.tab == Tab::Stage {
            if hit(ui, Modifiers::NONE, Key::Delete) {
                timeline::delete_selected(p);
            }
            if hit(ui, Modifiers::COMMAND, Key::D) {
                timeline::duplicate_selected(p);
            }
        }
    }

    fn top_bar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("VN Studio").strong().color(ACCENT));
            ui.separator();
            if ui.button("Новый").on_hover_text("Создать новый проект").clicked() {
                self.new_dialog = Some(NewDialog { name: "Моя новелла".into(), folder: config::projects_dir().display().to_string(), kind: NewKind::Manual });
            }
            if ui.button("Открыть").on_hover_text("Открыть папку проекта").clicked() {
                if let Some(dir) = rfd::FileDialog::new().set_directory(config::projects_dir()).pick_folder() {
                    self.open_project(&dir);
                }
            }
            let Some(p) = &mut self.project else {
                self.update_button(ui);
                return;
            };
            let mut save = false;
            let mut build = None;
            if ui.add_enabled(p.dirty(), egui::Button::new("Сохранить")).on_hover_text("Ctrl+S").clicked() {
                save = true;
            }
            ui.separator();
            if ui.add_enabled(p.history.can_undo(), egui::Button::new("↶ Отменить")).on_hover_text("Ctrl+Z").clicked() {
                p.undo();
            }
            if ui.add_enabled(p.history.can_redo(), egui::Button::new("↷ Повторить")).on_hover_text("Ctrl+Y").clicked() {
                p.redo();
            }
            ui.separator();
            if ui.button(RichText::new("▶ Предпросмотр").color(Color32::WHITE)).on_hover_text("Запустить игру с начала (F5)").clicked() {
                p.start_preview(false);
            }
            if ui.button("✔ Проверить").on_hover_text("Найти ошибки в истории").clicked() {
                p.check();
            }
            ui.menu_button("Собрать", |ui| {
                if ui.button("Игра для Windows (папка с .exe)").clicked() {
                    build = Some(Build::Desktop);
                    ui.close();
                }
                if ui.button("Версия для телефона (веб / PWA)").clicked() {
                    build = Some(Build::Web);
                    ui.close();
                }
                ui.separator();
                if ui.button("Выпустить обновление истории (.vnpak)…").clicked() {
                    p.release = Some((p.story.meta.version.clone(), true));
                    ui.close();
                }
            });
            if ui.button("✦ ИИ").on_hover_text("Создать или продолжить историю с помощью ИИ").clicked() {
                p.tab = Tab::Ai;
            }
            self.update_button(ui);
            if save {
                self.save();
            }
            if let Some(b) = build {
                self.build(b);
            }
        });
    }

    fn update_button(&mut self, ui: &mut Ui) {
        ui.separator();
        let busy = matches!(self.updater, Updater::Checking(_) | Updater::Installing(_));
        if ui.add_enabled(!busy, egui::Button::new("⟳ Обновить редактор")).on_hover_text(format!("Установлена версия {}. Проверить новую на GitHub.", update::CURRENT)).clicked() {
            self.check_updates();
        }
        if busy {
            ui.spinner();
        }
    }

    fn updater_window(&mut self, ctx: &egui::Context) {
        match &self.updater {
            Updater::Checking(rx) => {
                if let Ok(r) = rx.try_recv() {
                    self.updater = Updater::Shown(r);
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
            Updater::Installing(rx) => {
                if let Ok(r) = rx.try_recv() {
                    self.updater = match r {
                        Ok(()) => Updater::Installed,
                        Err(e) => Updater::Shown(Err(e)),
                    };
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
            Updater::Idle => return,
            _ => {}
        }
        let mut next = None;
        egui::Window::new("Обновление редактора").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.set_max_width(460.0);
            match &self.updater {
                Updater::Shown(Ok(Check::UpToDate)) => {
                    ui.label(format!("У вас последняя версия ({}).", update::CURRENT));
                }
                Updater::Shown(Ok(Check::Available(release))) => {
                    ui.label(format!("Доступна версия {} (установлена {}).", release.tag, update::CURRENT));
                    if !release.notes.is_empty() {
                        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| ui.label(&release.notes));
                    }
                    ui.label("Проекты и настройки обновление не затрагивает. Прежние файлы программы сохраняются с окончанием .old.");
                    if ui.button("Установить").clicked() {
                        let (tx, rx) = channel();
                        let release = release.clone();
                        std::thread::spawn(move || drop(tx.send(update::install(&release))));
                        next = Some(Updater::Installing(rx));
                    }
                }
                Updater::Shown(Err(e)) => {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
                Updater::Installed => {
                    ui.label("Обновление установлено. Оно начнёт работать после перезапуска редактора.");
                    if ui.button("Сохранить проект и перезапустить").clicked() {
                        self.wants_restart = true;
                    }
                }
                _ => {}
            }
            if ui.button("Закрыть").clicked() {
                next = Some(Updater::Idle);
            }
        });
        if let Some(n) = next {
            self.updater = n;
        }
        if self.wants_restart {
            self.save();
            if self.error.is_none() {
                match update::restart() {
                    Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(e) => self.error = Some(e),
                }
            }
            self.wants_restart = false;
            self.updater = Updater::Idle;
        }
    }

    fn welcome(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading("Visual Novel Studio");
            ui.label("Редактор визуальных новелл: сцены, диалоги, ветки сюжета, одежда, сборка игры для Windows и телефона.");
            ui.add_space(16.0);
            if ui.button(RichText::new("  Создать новый проект  ").size(18.0)).clicked() {
                self.new_dialog = Some(NewDialog { name: "Моя новелла".into(), folder: config::projects_dir().display().to_string(), kind: NewKind::Manual });
            }
            if ui.button("Создать демо-проект «Свет маяка»").on_hover_text("Готовая маленькая история, на которой видно все возможности").clicked() {
                self.new_dialog = Some(NewDialog { name: "Свет маяка".into(), folder: config::projects_dir().display().to_string(), kind: NewKind::Demo });
            }
            if !self.cfg.recent.is_empty() {
                ui.add_space(20.0);
                ui.strong("Недавние проекты");
                for dir in self.cfg.recent.clone() {
                    if ui.link(dir.display().to_string()).clicked() {
                        self.open_project(&dir);
                    }
                }
            }
        });
    }

    fn new_project_window(&mut self, ctx: &egui::Context) {
        let Some(d) = &mut self.new_dialog else { return };
        let mut create = false;
        let mut close = false;
        egui::Window::new("Новый проект").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            widgets::row(ui, "Название", |ui| ui.text_edit_singleline(&mut d.name));
            widgets::row(ui, "Папка", |ui| {
                ui.add(egui::TextEdit::singleline(&mut d.folder).desired_width(260.0));
                if ui.button("Обзор…").clicked() {
                    if let Some(f) = rfd::FileDialog::new().pick_folder() {
                        d.folder = f.display().to_string();
                    }
                }
            });
            ui.weak(format!("Проект будет создан в отдельной папке: {}", Path::new(&d.folder).join(vn_store::fsutil::safe_name(&d.name)).display()));
            ui.separator();
            ui.strong("Как создавать историю");
            ui.radio_value(&mut d.kind, NewKind::Manual, "Вручную — пустой проект с одной сценой");
            ui.radio_value(&mut d.kind, NewKind::Ai, "С помощью ИИ — облачного или локального (откроется вкладка «ИИ»)");
            ui.radio_value(&mut d.kind, NewKind::Demo, "Демо-проект — готовая история для знакомства");
            ui.separator();
            ui.horizontal(|ui| {
                create = ui.add_enabled(!d.name.trim().is_empty() && !d.folder.trim().is_empty(), egui::Button::new("Создать")).clicked();
                close = ui.button("Отмена").clicked();
            });
        });
        if create {
            let d = self.new_dialog.take().expect("диалог открыт");
            self.create_project(&Path::new(d.folder.trim()).join(vn_store::fsutil::safe_name(d.name.trim())), d.name.trim(), d.kind);
        } else if close {
            self.new_dialog = None;
        }
    }

    fn windows(&mut self, ctx: &egui::Context) {
        self.new_project_window(ctx);
        self.updater_window(ctx);
        if let Some(e) = self.error.clone() {
            egui::Window::new("Ошибка").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                ui.set_max_width(480.0);
                ui.label(e);
                if ui.button("Понятно").clicked() {
                    self.error = None;
                }
            });
        }
        let mut release = None;
        if let Some(p) = &mut self.project {
            extra::confirm_window(ctx, p);
            extra::built_window(ctx, p);
            extra::notes_window(ctx, p);
            release = extra::release_window(ctx, p);
        }
        if let Some((version, with_assets)) = release {
            self.release_update(&version, with_assets);
        }
    }

    fn dropped_files(&mut self, ui: &Ui) {
        let dropped: Vec<PathBuf> = ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        if dropped.is_empty() {
            return;
        }
        match &mut self.project {
            Some(p) => {
                p.import(p.import_kind, &dropped);
                p.left = LeftTab::Assets;
            }
            None => {
                if let Some(dir) = dropped.iter().find(|d| d.join("project.json").exists()) {
                    self.open_project(&dir.clone());
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.hotkeys(ui);
        self.dropped_files(ui);
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(3.0);
            self.top_bar(ui);
            ui.add_space(3.0);
        });
        if self.project.is_none() {
            egui::CentralPanel::default().show(ui, |ui| self.welcome(ui));
            self.windows(&ctx);
            return;
        }
        let p = self.project.as_mut().expect("проект открыт");
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("{}{}", p.story.meta.name, if p.dirty() { " •" } else { "" }));
                ui.separator();
                ui.weak(format!("сцен: {}  ·  версия {} (выпуск {})", p.story.scenes.len(), p.story.meta.version, p.story.meta.story_version));
                ui.separator();
                ui.label(&p.status);
            });
        });
        if p.issues.is_some() {
            egui::Panel::bottom("issues").resizable(true).default_size(150.0).show(ui, |ui| extra::issues_ui(ui, p));
        }
        egui::Panel::left("left").resizable(true).default_size(270.0).min_size(220.0).show(ui, |ui| left::ui(ui, p));
        if p.tab == Tab::Stage {
            egui::Panel::right("timeline").resizable(true).default_size(330.0).min_size(260.0).show(ui, |ui| timeline::ui(ui, p));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                for (tab, name) in [(Tab::Stage, "Сцена"), (Tab::Graph, "⇄ Сюжет"), (Tab::Preview, "▶ Предпросмотр"), (Tab::Wardrobe, "Гардероб"), (Tab::Endings, "⚑ Концовки"), (Tab::Ai, "✦ ИИ")] {
                    if ui.selectable_label(p.tab == tab, name).clicked() {
                        p.tab = tab;
                    }
                }
            });
            ui.separator();
            match p.tab {
                Tab::Stage => {
                    egui::Panel::bottom("props").resizable(true).default_size(240.0).min_size(200.0).show(ui, |ui| props::ui(ui, p));
                    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| stage::ui(ui, p));
                }
                Tab::Graph => graph::ui(ui, p),
                Tab::Preview => extra::preview_ui(ui, p),
                Tab::Wardrobe => extra::wardrobe_ui(ui, p),
                Tab::Endings => extra::endings_ui(ui, p),
                Tab::Ai => {
                    if self.ai.ui(ui, &mut self.cfg.ai, p) {
                        self.cfg.save(&self.cfg_path);
                    }
                }
            }
        });
        let now = ui.input(|i| i.time);
        if let Some(p) = &mut self.project {
            if p.tab != Tab::Preview {
                // Игра вне своей вкладки не идёт: иначе её музыка звучала бы поверх редактора.
                p.game = None;
            }
            p.audio.update(ui.input(|i| i.stable_dt), 1.0);
            p.after_frame(now);
        }
        self.windows(&ctx);
    }
}
