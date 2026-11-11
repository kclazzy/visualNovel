//! Вкладка «ИИ»: подключение, мини-инструкция, создание и продолжение истории.
use crate::ai::{self, AiSettings, Protocol, PRESETS};
use crate::app::Project;
use crate::ops;
use crate::widgets::row;
use egui::{Color32, RichText, Ui};
use std::sync::mpsc::{channel, Receiver};
use vn_core::draft::{self, Draft};
use vn_core::model::SceneId;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    NewStory,
    Continue,
}

pub struct AiPanel {
    pub mode: Mode,
    pub idea: String,
    pub wish: String,
    pub scenes: u32,
    pub language: String,
    pub models: Vec<String>,
    /// Черновик, ожидающий решения автора, и то, куда он предназначен.
    pub draft: Option<(Draft, Mode, Option<SceneId>)>,
    pub message: Option<Result<String, String>>,
    job: Option<(Receiver<Result<String, String>>, Mode, Option<SceneId>)>,
    models_job: Option<Receiver<Result<Vec<String>, String>>>,
    show_key: bool,
}

impl Default for AiPanel {
    fn default() -> Self {
        Self { mode: Mode::NewStory, idea: String::new(), wish: String::new(), scenes: 6, language: "русский".into(), models: vec![], draft: None, message: None, job: None, models_job: None, show_key: false }
    }
}

impl AiPanel {
    /// Принять ответ модели: разобрать и показать автору, ничего не меняя в проекте.
    pub fn receive(&mut self, reply: Result<String, String>, mode: Mode, scene: Option<SceneId>) {
        match reply.and_then(|text| draft::parse_draft(&text)) {
            Ok(d) => {
                self.message = None;
                self.draft = Some((d, mode, scene));
            }
            Err(e) => self.message = Some(Err(e)),
        }
    }

    /// Добавить просмотренный черновик в проект.
    pub fn accept(&mut self, p: &mut Project) {
        let Some((d, mode, scene)) = self.draft.take() else { return };
        match (mode, scene) {
            (Mode::Continue, Some(scene)) => {
                let n = draft::append_lines(&mut p.story, &scene, &d);
                self.message = Some(Ok(format!("В сцену добавлено реплик: {n}. Отменить — Ctrl+Z.")));
                p.select(Some(scene), None);
            }
            _ => {
                let blank = (p.story.scenes.len() == 1 && p.story.scenes[0].events.is_empty()).then(|| p.story.scenes[0].id.clone());
                let new = draft::apply_draft(&mut p.story, &d);
                if let (Some(old), Some(first)) = (blank, new.first()) {
                    p.story.remove_scene(&old, None);
                    p.story.redirects.remove(&old);
                    p.story.meta.start_scene = Some(first.clone());
                }
                ops::auto_layout(&mut p.story, false);
                self.message = Some(Ok(format!("Добавлено сцен: {}. Проверьте историю кнопкой «Проверить»; отменить — Ctrl+Z.", new.len())));
                p.select(new.first().cloned(), None);
            }
        }
        p.tab = crate::app::Tab::Graph;
        p.graph_view = egui::Rect::ZERO;
    }

    fn poll(&mut self, ui: &Ui) {
        if let Some((rx, mode, scene)) = &self.job {
            if let Ok(reply) = rx.try_recv() {
                let (mode, scene) = (*mode, scene.clone());
                self.job = None;
                self.receive(reply, mode, scene);
            }
        }
        if let Some(rx) = &self.models_job {
            if let Ok(reply) = rx.try_recv() {
                self.models_job = None;
                match reply {
                    Ok(m) if m.is_empty() => self.message = Some(Err("Сервер ответил, но моделей нет. Для Ollama скачайте модель командой «ollama pull …».".into())),
                    Ok(m) => {
                        self.message = Some(Ok(format!("Подключение работает. Доступно моделей: {}.", m.len())));
                        self.models = m;
                    }
                    Err(e) => self.message = Some(Err(e)),
                }
            }
        }
        if self.job.is_some() || self.models_job.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn settings_ui(&mut self, ui: &mut Ui, s: &mut AiSettings) {
        ui.strong("Подключение ИИ");
        let before = s.preset;
        let mut preset = s.preset.min(PRESETS.len() - 1);
        egui::ComboBox::from_id_salt("ai-preset").selected_text(PRESETS[preset].name).width(250.0).show_ui(ui, |ui| {
            for (i, p) in PRESETS.iter().enumerate() {
                ui.selectable_value(&mut preset, i, p.name);
            }
        });
        if preset != before {
            s.use_preset(preset);
            self.models.clear();
        }
        row(ui, "Адрес сервера", |ui| ui.add(egui::TextEdit::singleline(&mut s.base_url).desired_width(250.0).hint_text("http://…/v1")));
        if !s.is_local() {
            row(ui, "Ключ API", |ui| {
                ui.add(egui::TextEdit::singleline(&mut s.api_key).password(!self.show_key).desired_width(210.0));
                ui.toggle_value(&mut self.show_key, "●").on_hover_text("Показать ключ");
            });
        }
        row(ui, "Модель", |ui| {
            ui.add(egui::TextEdit::singleline(&mut s.model).desired_width(170.0).hint_text("название модели"));
            if !self.models.is_empty() {
                ui.menu_button("▾ выбрать", |ui| {
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for m in &self.models {
                            if ui.button(m).clicked() {
                                s.model = m.clone();
                                ui.close();
                            }
                        }
                    });
                });
            }
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(self.models_job.is_none(), egui::Button::new("Список моделей / проверить подключение")).clicked() {
                let (tx, rx) = channel();
                let settings = s.clone();
                std::thread::spawn(move || drop(tx.send(ai::list_models(&settings))));
                self.models_job = Some(rx);
            }
            if self.models_job.is_some() {
                ui.spinner();
            }
        });
        if s.protocol == Protocol::Anthropic && s.preset != 4 {
            ui.weak("Протокол: Anthropic");
        }
        ui.add_space(6.0);
        egui::CollapsingHeader::new(RichText::new("Мини-инструкция: как подключить ИИ").strong()).default_open(true).show(ui, |ui| {
            ui.label(RichText::new(ai::GUIDE).size(12.5));
        });
    }

    fn draft_ui(&mut self, ui: &mut Ui, p: &mut Project) {
        let Some((d, mode, _)) = &self.draft else { return };
        ui.separator();
        ui.strong("Черновик от ИИ — посмотрите перед добавлением");
        egui::ScrollArea::vertical().id_salt("draft").max_height(ui.available_height() - 50.0).show(ui, |ui| {
            if !d.characters.is_empty() && *mode == Mode::NewStory {
                ui.label(format!("Персонажи: {}", d.characters.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")));
            }
            for s in &d.scenes {
                egui::CollapsingHeader::new(format!("{} — реплик: {}", s.name, s.lines.len())).id_salt(&s.key).default_open(d.scenes.len() == 1).show(ui, |ui| {
                    for l in &s.lines {
                        ui.label(match &l.speaker {
                            Some(who) => format!("{who}: {}", l.text),
                            None => l.text.clone(),
                        });
                    }
                    for c in &s.choices {
                        ui.label(RichText::new(format!("➤ {}", c.text)).color(crate::app::ACCENT));
                    }
                    if let Some(e) = &s.ending {
                        ui.label(RichText::new(format!("⚑ Концовка: {e}")).color(vn_view::draw::GOLD));
                    }
                });
            }
        });
        let mut accept = false;
        ui.horizontal(|ui| {
            accept = ui.button(RichText::new("✔ Добавить в проект").color(Color32::WHITE)).clicked();
            if ui.button("Отклонить").clicked() {
                self.draft = None;
            }
        });
        if accept {
            self.accept(p);
        }
    }

    fn generate_ui(&mut self, ui: &mut Ui, s: &AiSettings, p: &mut Project) {
        ui.strong("Что сделать");
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.mode, Mode::NewStory, "Новая история");
            ui.radio_value(&mut self.mode, Mode::Continue, "Продолжить выбранную сцену");
        });
        match self.mode {
            Mode::NewStory => {
                ui.add(egui::TextEdit::multiline(&mut self.idea).desired_rows(5).desired_width(f32::INFINITY).hint_text("Идея истории: жанр, место, герои, чем всё может закончиться.\nНапример: «Детектив в приморском городке. Журналистка Вера ищет пропавшего смотрителя маяка. Три концовки»."));
                row(ui, "Сцен", |ui| ui.add(egui::Slider::new(&mut self.scenes, 2..=20)));
            }
            Mode::Continue => {
                ui.label(format!("Сцена: {}", p.current_scene().map(|s| s.name.as_str()).unwrap_or("не выбрана")));
                ui.add(egui::TextEdit::multiline(&mut self.wish).desired_rows(3).desired_width(f32::INFINITY).hint_text("Что должно произойти дальше (необязательно)"));
            }
        }
        row(ui, "Язык текста", |ui| ui.add(egui::TextEdit::singleline(&mut self.language).desired_width(120.0)));
        let scene = p.current_scene().cloned();
        let problem = s.missing().or(match self.mode {
            Mode::NewStory if self.idea.trim().is_empty() => Some("Опишите идею истории."),
            Mode::Continue if scene.is_none() => Some("Выберите сцену в левой панели."),
            _ => None,
        });
        ui.horizontal(|ui| {
            if ui.add_enabled(problem.is_none() && self.job.is_none(), egui::Button::new(RichText::new("✦ Сгенерировать").color(Color32::WHITE))).clicked() {
                let user = match (self.mode, &scene) {
                    (Mode::Continue, Some(sc)) => draft::continue_prompt(&p.story, sc, &self.wish, &self.language),
                    _ => draft::story_prompt(&self.idea, self.scenes, &self.language),
                };
                let (tx, rx) = channel();
                let settings = s.clone();
                std::thread::spawn(move || drop(tx.send(ai::ask(&settings, draft::SYSTEM_PROMPT, &user))));
                self.job = Some((rx, self.mode, scene.map(|s| s.id)));
                self.message = None;
            }
            if self.job.is_some() {
                ui.spinner();
                ui.label("ИИ пишет… Локальной модели может понадобиться несколько минут.");
                if ui.button("Отменить").clicked() {
                    self.job = None;
                }
            } else if let Some(why) = problem {
                ui.weak(why);
            }
        });
        match &self.message {
            Some(Ok(m)) => drop(ui.colored_label(Color32::LIGHT_GREEN, m)),
            Some(Err(e)) => drop(ui.colored_label(Color32::LIGHT_RED, e)),
            None => {}
        }
        self.draft_ui(ui, p);
    }

    /// Возвращает `true`, если настройки подключения изменились и их нужно сохранить.
    pub fn ui(&mut self, ui: &mut Ui, s: &mut AiSettings, p: &mut Project) -> bool {
        self.poll(ui);
        let before = s.clone();
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical().id_salt("ai-left").show(&mut cols[0], |ui| self.settings_ui(ui, s));
            self.generate_ui(&mut cols[1], s, p);
        });
        *s != before
    }
}
