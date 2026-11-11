//! Игра целиком: главное меню, сцена, диалоги, выборы, сохранения, настройки, гардероб.
//! Рисует `View` движка и передаёт ему действия игрока; сюжетных решений здесь нет.
use crate::assets::{Assets, Textures};
use crate::audio::Audio;
use crate::draw;
use egui::{vec2, Align, Color32, CornerRadius, FontId, Key, Layout, Rect, RichText, Sense, Ui, UiBuilder};
use std::sync::Arc;
use vn_core::model::{SceneId, SlotId, Story};
use vn_core::runtime::Runtime;
use vn_core::save::Restored;
use vn_core::view::{Prompt, Sprite, View};
use vn_store::fsutil::{human_time, now};
use vn_store::saves::{slot_name, PlayerSettings, SaveStore, AUTO, QUICK, SLOTS};

#[derive(Clone, PartialEq, Debug)]
enum Screen {
    Title,
    Game,
    Menu,
    Saves { saving: bool, back: Box<Screen> },
    Settings { back: Box<Screen> },
    Log,
    Wardrobe { slots: Vec<SlotId>, blocking: bool },
}

#[derive(Clone, Debug, PartialEq)]
struct SpriteAnim {
    sprite: Sprite,
    from: [f32; 3],
    cur: [f32; 3],
    t: f32,
    alpha: f32,
}

fn target(s: &Sprite) -> [f32; 3] {
    [s.x, s.y, s.height]
}

/// Плавные переходы сцены: смена фона, появление, движение и исчезновение спрайтов, затемнение.
#[derive(Default, Debug)]
pub struct Anim {
    bg: Option<String>,
    bg_prev: Option<String>,
    bg_t: f32,
    bg_fade: f32,
    sprites: Vec<SpriteAnim>,
    leaving: Vec<(SpriteAnim, f32)>,
    black: f32,
    black_target: f32,
    black_fade: f32,
}

fn toward(value: f32, goal: f32, seconds: f32, dt: f32) -> f32 {
    crate::audio::ramp(value, goal, if seconds > 0.0 { 1.0 / seconds } else { 0.0 }, dt)
}

impl Anim {
    /// Принять новую картину движка. `instant` — без переходов (загрузка сохранения).
    pub fn retarget(&mut self, view: &View, instant: bool) {
        if view.background != self.bg {
            self.bg_prev = self.bg.take();
            self.bg = view.background.clone();
            self.bg_fade = view.bg_fade;
            self.bg_t = if instant || view.bg_fade <= 0.0 { 1.0 } else { 0.0 };
        }
        let mut old = std::mem::take(&mut self.sprites);
        for s in &view.sprites {
            let known = old.iter().position(|a| a.sprite.id == s.id).map(|i| old.remove(i));
            self.sprites.push(match known {
                Some(mut a) => {
                    if target(&a.sprite) != target(s) {
                        a.from = a.cur;
                        a.t = if instant || s.move_time <= 0.0 { 1.0 } else { 0.0 };
                    }
                    a.sprite = s.clone();
                    if a.t >= 1.0 {
                        a.cur = target(s);
                    }
                    if instant {
                        a.alpha = 1.0;
                    }
                    a
                }
                None => SpriteAnim { sprite: s.clone(), from: target(s), cur: target(s), t: 1.0, alpha: if instant || s.fade <= 0.0 { 1.0 } else { 0.0 } },
            });
        }
        if instant {
            self.leaving.clear();
        } else if view.hide_fade > 0.0 {
            self.leaving.extend(old.into_iter().map(|a| (a, view.hide_fade)));
        }
        self.black_target = if view.black { 1.0 } else { 0.0 };
        self.black_fade = view.fade;
        if instant {
            self.black = self.black_target;
        }
    }

    /// Шаг времени. `true`, пока что-то ещё движется.
    pub fn step(&mut self, dt: f32) -> bool {
        self.bg_t = toward(self.bg_t, 1.0, self.bg_fade, dt);
        for a in &mut self.sprites {
            a.t = toward(a.t, 1.0, a.sprite.move_time, dt);
            let ease = a.t * a.t * (3.0 - 2.0 * a.t);
            let goal = target(&a.sprite);
            a.cur = std::array::from_fn(|i| a.from[i] + (goal[i] - a.from[i]) * ease);
            a.alpha = toward(a.alpha, 1.0, a.sprite.fade, dt);
        }
        for (a, fade) in &mut self.leaving {
            a.alpha = toward(a.alpha, 0.0, *fade, dt);
        }
        self.leaving.retain(|(a, _)| a.alpha > 0.0);
        self.black = toward(self.black, self.black_target, self.black_fade, dt);
        self.bg_t < 1.0 || self.black != self.black_target || !self.leaving.is_empty() || self.sprites.iter().any(|a| a.t < 1.0 || a.alpha < 1.0)
    }
}

pub struct GameView {
    story: Story,
    assets: Arc<dyn Assets>,
    saves: SaveStore,
    pub settings: PlayerSettings,
    rt: Option<Runtime>,
    view: Option<View>,
    screen: Screen,
    textures: Textures,
    audio: Audio,
    anim: Anim,
    typed: f32,
    wait_left: f32,
    auto_left: f32,
    auto: bool,
    skip: bool,
    hide_ui: bool,
    toast: Option<(String, f64)>,
    /// Игрок нажал «Выход» в главном меню.
    pub wants_quit: bool,
    /// Показывать пункт «Выход» (в предпросмотре редактора он не нужен).
    pub can_quit: bool,
    /// Остановить время (кнопка «Пауза» предпросмотра).
    pub paused: bool,
}

impl GameView {
    pub fn new(story: Story, assets: Arc<dyn Assets>, saves: SaveStore) -> Self {
        let settings = saves.settings();
        Self {
            story,
            textures: Textures::new(assets.clone()),
            assets,
            saves,
            settings,
            rt: None,
            view: None,
            screen: Screen::Title,
            audio: Audio::default(),
            anim: Anim::default(),
            typed: 0.0,
            wait_left: 0.0,
            auto_left: 0.0,
            auto: false,
            skip: false,
            hide_ui: false,
            toast: None,
            wants_quit: false,
            can_quit: true,
            paused: false,
        }
    }

    pub fn runtime(&self) -> Option<&Runtime> {
        self.rt.as_ref()
    }

    pub fn story(&self) -> &Story {
        &self.story
    }

    /// Начать игру: с начала истории или с выбранной сцены.
    pub fn start(&mut self, scene: Option<SceneId>) {
        self.anim = Anim::default();
        self.audio.stop_all();
        self.rt = Some(match scene {
            Some(s) => Runtime::start_at(self.story.clone(), s),
            None => Runtime::new(self.story.clone()),
        });
        self.auto = false;
        self.skip = false;
        self.screen = Screen::Game;
        self.refresh(false);
    }

    /// Вернуться в главное меню.
    pub fn to_title(&mut self) {
        self.rt = None;
        self.view = None;
        self.audio.stop_all();
        self.screen = Screen::Title;
    }

    fn say(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), 0.0));
    }

    fn play_sound(&mut self, file: &str, volume: f32) {
        if let Some(bytes) = self.assets.read(file) {
            self.audio.play_sound(bytes, volume * self.settings.sfx);
        }
    }

    /// Пересчитать картину после действия. `instant` — без переходов и звуков.
    fn refresh(&mut self, instant: bool) {
        let Some(rt) = &mut self.rt else { return };
        let view = rt.view();
        let cues = rt.take_cues();
        if !instant {
            for c in cues {
                self.play_sound(&c.sound, c.volume);
            }
            if let Prompt::Text { sound: Some(s), .. } = &view.prompt {
                self.play_sound(&s.clone(), 1.0);
            }
        }
        let assets = self.assets.clone();
        self.audio.set_music(view.music.as_ref().map(|m| (m.track.as_str(), m.volume, m.looped)), if instant { 0.0 } else { view.music_fade }, &|p| assets.read(p));
        self.typed = 0.0;
        self.auto_left = 0.0;
        self.wait_left = 0.0;
        match &view.prompt {
            Prompt::Wait { seconds } => self.wait_left = *seconds,
            Prompt::Wardrobe { slots } => self.screen = Screen::Wardrobe { slots: slots.clone(), blocking: true },
            Prompt::Choice { .. } => self.skip = false,
            Prompt::Ended { .. } | Prompt::Error { .. } => {
                self.skip = false;
                self.auto = false;
            }
            Prompt::Text { .. } => {}
        }
        self.anim.retarget(&view, instant);
        self.view = Some(view);
    }

    fn write_save(&mut self, slot: &str) -> bool {
        let Some(rt) = &self.rt else { return false };
        match self.saves.write(slot, &rt.save(now())) {
            Ok(()) => true,
            Err(e) => {
                self.say(e);
                false
            }
        }
    }

    fn advance(&mut self) {
        let was_text = matches!(self.view.as_ref().map(|v| &v.prompt), Some(Prompt::Text { .. }));
        if let Some(rt) = &mut self.rt {
            rt.advance();
        }
        if was_text {
            self.write_save(AUTO);
        }
        self.refresh(false);
    }

    /// Загрузить слот. Сообщает игроку, если история с тех пор изменилась.
    pub fn load_slot(&mut self, slot: &str) {
        let save = match self.saves.read(slot) {
            None => return self.say("Слот пуст"),
            Some(Err(e)) => return self.say(e.to_string()),
            Some(Ok(s)) => s,
        };
        self.load(save);
    }

    fn load(&mut self, save: vn_core::save::SaveFile) {
        match Runtime::restore(self.story.clone(), save) {
            Err(e) => self.say(e.to_string()),
            Ok((rt, how)) => {
                self.rt = Some(rt);
                self.anim = Anim::default();
                self.audio.stop_all();
                self.auto = false;
                self.skip = false;
                self.screen = Screen::Game;
                self.refresh(true);
                match how {
                    Restored::Exact => {}
                    Restored::SceneRestart => self.say("История обновилась: продолжаем с начала сцены"),
                    Restored::Redirected(_) => self.say("История обновилась: сцена изменена, продолжаем с новой"),
                }
            }
        }
    }

    /// Щелчок / пробел: показать текст целиком или перейти дальше.
    pub fn tap(&mut self) {
        if self.hide_ui {
            self.hide_ui = false;
            return;
        }
        let Some(Prompt::Text { spans, .. }) = self.view.as_ref().map(|v| &v.prompt) else { return };
        let total = draw::char_count(spans) as f32;
        if self.typed < total {
            self.typed = total;
        } else {
            self.advance();
        }
    }

    fn toggle_skip(&mut self) {
        let seen = matches!(self.view.as_ref().map(|v| &v.prompt), Some(Prompt::Text { seen: true, .. }));
        if !self.skip && !seen {
            self.say("Этот текст ещё не прочитан");
        } else {
            self.skip = !self.skip;
        }
    }

    // ---------- отрисовка ----------

    pub fn ui(&mut self, ui: &mut Ui) {
        let avail = ui.available_rect_before_wrap();
        let stage = draw::fit(avail, self.story.meta.resolution);
        ui.painter().rect_filled(avail, CornerRadius::ZERO, Color32::BLACK);
        let click = ui.allocate_rect(avail, Sense::click());
        let (dt, time) = ui.input(|i| (i.stable_dt.min(0.1), i.time));
        let dt = if self.paused { 0.0 } else { dt };
        self.textures.collect_garbage(time);

        // Экран фиксируется на начало кадра: клавиша, открывшая меню, не должна тут же его закрыть.
        let screen = self.screen.clone();
        if self.rt.is_some() {
            let running = screen == Screen::Game;
            self.scene(ui, stage, if running { dt } else { 0.0 });
            if running {
                self.play(ui, stage, dt, click.clicked());
            }
        } else {
            ui.painter().rect_filled(stage, CornerRadius::ZERO, Color32::from_rgb(17, 18, 40));
        }
        if self.audio.update(dt, self.settings.music) {
            ui.ctx().request_repaint();
        }
        match screen {
            Screen::Game => {}
            Screen::Title => self.title(ui, stage),
            Screen::Menu => self.menu(ui, stage),
            Screen::Saves { saving, back } => self.saves_screen(ui, stage, saving, *back),
            Screen::Settings { back } => self.settings_screen(ui, stage, *back),
            Screen::Log => self.log_screen(ui, stage),
            Screen::Wardrobe { slots, blocking } => self.wardrobe_screen(ui, stage, &slots, blocking),
        }
        self.draw_toast(ui, stage, time);
    }

    /// Фон, спрайты и затемнение.
    fn scene(&mut self, ui: &mut Ui, stage: Rect, dt: f32) {
        let painter = ui.painter_at(stage);
        let ctx = ui.ctx().clone();
        if self.anim.step(dt) {
            ctx.request_repaint();
        }
        let a = &self.anim;
        if let (Some(prev), true) = (&a.bg_prev, a.bg_t < 1.0) {
            draw::background(&painter, &ctx, &mut self.textures, stage, prev, 1.0);
        }
        if let Some(bg) = &a.bg {
            draw::background(&painter, &ctx, &mut self.textures, stage, bg, a.bg_t);
        }
        for s in a.leaving.iter().map(|(s, _)| s).chain(&a.sprites) {
            draw::sprite(&painter, &ctx, &mut self.textures, stage, &s.sprite.layers, s.cur, s.alpha);
        }
        if a.black > 0.0 {
            painter.rect_filled(stage, CornerRadius::ZERO, Color32::from_black_alpha((a.black * 255.0) as u8));
        }
    }

    /// Диалог, выбор, паузы и управление во время игры.
    fn play(&mut self, ui: &mut Ui, stage: Rect, dt: f32, clicked: bool) {
        let Some(view) = self.view.clone() else { return };
        let unit = stage.height();
        let (ctrl, keys) = ui.input(|i| (i.modifiers.ctrl, [Key::Space, Key::Enter, Key::Escape, Key::F5, Key::F9, Key::H, Key::A].map(|k| i.key_pressed(k))));
        let skipping = self.skip || ctrl;
        let mut tap = clicked || keys[0] || keys[1];

        match &view.prompt {
            Prompt::Text { speaker, style, spans, speed, seen, .. } => {
                let total = draw::char_count(spans) as f32;
                if skipping && *seen || self.settings.text_speed >= 5.0 {
                    self.typed = total;
                }
                if self.typed < total {
                    self.typed = (self.typed + dt * speed * self.settings.text_speed).min(total);
                    ui.ctx().request_repaint();
                } else if skipping && *seen {
                    tap = true;
                    ui.ctx().request_repaint();
                } else if self.auto && !self.paused {
                    self.auto_left += dt;
                    ui.ctx().request_repaint();
                    if self.auto_left > self.settings.auto_delay + total * 0.025 {
                        tap = true;
                    }
                } else if self.skip {
                    self.skip = false;
                }
                if !self.hide_ui {
                    draw::dialog(&ui.painter_at(stage), stage, view.dialog, &draw::Line { speaker: speaker.as_ref(), style: *style, spans, shown: self.typed as usize }, &self.story.meta.text);
                }
            }
            Prompt::Choice { prompt, options } => {
                if !self.hide_ui {
                    let area = Rect::from_center_size(stage.center() - vec2(0.0, unit * 0.08), vec2(stage.width() * 0.7, unit * 0.6));
                    let mut chosen = None;
                    ui.scope_builder(UiBuilder::new().max_rect(area).layout(Layout::top_down(Align::Center)), |ui| {
                        ui.add_space((area.height() - (options.len() as f32 + 1.0) * unit * 0.095).max(0.0) / 2.0);
                        if !prompt.is_empty() {
                            ui.label(RichText::new(prompt).size(unit * 0.042).color(Color32::WHITE).strong());
                            ui.add_space(unit * 0.02);
                        }
                        for (i, o) in options.iter().enumerate() {
                            let label = RichText::new(format!("{}. {}", i + 1, o.text)).size(unit * 0.036).color(Color32::WHITE);
                            let button = egui::Button::new(label).fill(Color32::from_rgb(52, 70, 150)).stroke(egui::Stroke::new(2.0, Color32::from_rgb(232, 194, 106))).corner_radius(CornerRadius::same((unit * 0.03) as u8)).min_size(vec2(area.width(), unit * 0.075));
                            if ui.add(button).clicked() {
                                chosen = Some(o.id.clone());
                            }
                            ui.add_space(unit * 0.012);
                        }
                    });
                    if let (Some(id), Some(rt)) = (chosen, &mut self.rt) {
                        if rt.choose(&id) {
                            self.write_save(AUTO);
                            self.refresh(false);
                        }
                        return;
                    }
                }
            }
            Prompt::Wait { .. } => {
                self.wait_left -= dt;
                ui.ctx().request_repaint();
                if self.wait_left <= 0.0 || skipping {
                    self.advance();
                    return;
                }
            }
            Prompt::Wardrobe { .. } => {}
            Prompt::Ended { title } => {
                let title = if title.is_empty() { "Конец" } else { title.as_str() };
                self.overlay(ui, stage, title, |me, ui| {
                    ui.label("Спасибо за игру!");
                    ui.add_space(8.0);
                    if ui.button("История диалогов").clicked() {
                        me.screen = Screen::Log;
                    }
                    if ui.button("В главное меню").clicked() {
                        me.to_title();
                    }
                });
                return;
            }
            Prompt::Error { message } => {
                let message = message.clone();
                self.overlay(ui, stage, "Ошибка в истории", |me, ui| {
                    ui.label(message);
                    ui.add_space(8.0);
                    if ui.button("В главное меню").clicked() {
                        me.to_title();
                    }
                });
                return;
            }
        }

        if !self.hide_ui {
            self.hud(ui, stage);
        }
        if keys[2] {
            self.screen = Screen::Menu;
        } else if keys[3] {
            if self.write_save(QUICK) {
                self.say("Сохранено");
            }
        } else if keys[4] {
            self.load_slot(QUICK);
        } else if keys[5] {
            self.hide_ui = !self.hide_ui;
        } else if keys[6] {
            self.auto = !self.auto;
        } else if tap {
            self.tap();
        }
    }

    fn hud(&mut self, ui: &mut Ui, stage: Rect) {
        let unit = stage.height();
        let bar = Rect::from_min_max(stage.right_top() + vec2(-stage.width() * 0.75, unit * 0.012), stage.right_top() + vec2(-unit * 0.012, unit * 0.07));
        ui.scope_builder(UiBuilder::new().max_rect(bar).layout(Layout::right_to_left(Align::Center)), |ui| {
            ui.style_mut().override_font_id = Some(FontId::proportional(unit * 0.026));
            let button = |ui: &mut Ui, text: &str, on: bool, hint: &str| ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE)).fill(if on { draw::ACCENT } else { draw::PANEL })).on_hover_text(hint).clicked();
            if button(ui, "Скрыть", false, "Скрыть интерфейс (H)") {
                self.hide_ui = true;
            }
            if button(ui, "Б.загр", false, "Быстрая загрузка (F9)") {
                self.load_slot(QUICK);
            }
            if button(ui, "Б.сохр", false, "Быстрое сохранение (F5)") && self.write_save(QUICK) {
                self.say("Сохранено");
            }
            if button(ui, "История", false, "История диалогов") {
                self.screen = Screen::Log;
            }
            if button(ui, "Пропуск", self.skip, "Пропуск прочитанного текста (или удерживайте Ctrl)") {
                self.toggle_skip();
            }
            if button(ui, "Авто", self.auto, "Автоматическое чтение (A)") {
                self.auto = !self.auto;
            }
            if button(ui, "Меню", false, "Меню (Esc)") {
                self.screen = Screen::Menu;
            }
        });
    }

    /// Затемнённая панель поверх сцены с заголовком и содержимым.
    fn overlay(&mut self, ui: &mut Ui, stage: Rect, title: &str, body: impl FnOnce(&mut Self, &mut Ui)) {
        let unit = stage.height();
        ui.painter_at(stage).rect_filled(stage, CornerRadius::ZERO, Color32::from_rgba_premultiplied(6, 7, 14, 225));
        let inner = stage.shrink2(vec2(stage.width() * 0.07, unit * 0.06));
        ui.scope_builder(UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)), |ui| {
            ui.visuals_mut().override_text_color = Some(Color32::from_gray(235));
            ui.style_mut().override_font_id = Some(FontId::proportional(unit * 0.034));
            ui.spacing_mut().button_padding = vec2(unit * 0.025, unit * 0.012);
            ui.spacing_mut().item_spacing = vec2(unit * 0.015, unit * 0.014);
            ui.spacing_mut().slider_width = stage.width() * 0.35;
            ui.label(RichText::new(title).size(unit * 0.06).strong().color(Color32::WHITE));
            ui.add_space(unit * 0.02);
            body(self, ui);
        });
    }

    fn title(&mut self, ui: &mut Ui, stage: Rect) {
        let (name, sub) = (self.story.meta.name.clone(), [self.story.meta.author.clone(), format!("версия {}", self.story.meta.version)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "));
        let latest = self.saves.latest();
        self.overlay(ui, stage, &name, |me, ui| {
            ui.label(RichText::new(sub).weak());
            ui.add_space(stage.height() * 0.04);
            if ui.button("Новая игра").clicked() {
                me.start(None);
            }
            if ui.add_enabled(latest.is_some(), egui::Button::new("Продолжить")).clicked() {
                if let Some(s) = latest {
                    me.load(s);
                }
            }
            if ui.button("Загрузить").clicked() {
                me.screen = Screen::Saves { saving: false, back: Box::new(Screen::Title) };
            }
            if ui.button("Настройки").clicked() {
                me.screen = Screen::Settings { back: Box::new(Screen::Title) };
            }
            if me.can_quit && ui.button("Выход").clicked() {
                me.wants_quit = true;
            }
        });
    }

    fn menu(&mut self, ui: &mut Ui, stage: Rect) {
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            self.screen = Screen::Game;
            return;
        }
        let wardrobe = self.story.wardrobe.anytime && self.story.wardrobe.hero.is_some();
        self.overlay(ui, stage, "Пауза", |me, ui| {
            if ui.button("Вернуться в игру").clicked() {
                me.screen = Screen::Game;
            }
            if ui.button("Сохранить").clicked() {
                me.screen = Screen::Saves { saving: true, back: Box::new(Screen::Menu) };
            }
            if ui.button("Загрузить").clicked() {
                me.screen = Screen::Saves { saving: false, back: Box::new(Screen::Menu) };
            }
            if wardrobe && ui.button("Гардероб").clicked() {
                me.screen = Screen::Wardrobe { slots: vec![], blocking: false };
            }
            if ui.button("История диалогов").clicked() {
                me.screen = Screen::Log;
            }
            if ui.button("Настройки").clicked() {
                me.screen = Screen::Settings { back: Box::new(Screen::Menu) };
            }
            if ui.button("В главное меню").clicked() {
                me.to_title();
            }
        });
    }

    fn back_button(&mut self, ui: &mut Ui, back: Screen) {
        if ui.button("← Назад").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
            self.screen = back;
        }
    }

    fn saves_screen(&mut self, ui: &mut Ui, stage: Rect, saving: bool, back: Screen) {
        let unit = stage.height();
        self.overlay(ui, stage, if saving { "Сохранить" } else { "Загрузить" }, |me, ui| {
            me.back_button(ui, back.clone());
            let names: Vec<(String, String)> = [(AUTO.to_string(), "Автосохранение".to_string()), (QUICK.to_string(), "Быстрое сохранение".to_string())].into_iter().filter(|_| !saving).chain((1..=SLOTS).map(|n| (slot_name(n), format!("Слот {n}")))).collect();
            let card = vec2(ui.available_width() / 2.0 - unit * 0.02, unit * 0.115);
            egui::Grid::new("slots").num_columns(2).show(ui, |ui| {
                for (i, (slot, title)) in names.iter().enumerate() {
                    let text = match me.saves.read(slot) {
                        None => format!("{title}\nПусто"),
                        Some(Err(e)) => format!("{title}\n{e}"),
                        Some(Ok(s)) => format!("{title} · {}\n{}", human_time(s.saved_at), s.label),
                    };
                    let button = egui::Button::new(RichText::new(text).size(unit * 0.026).color(Color32::from_rgb(32, 48, 79))).fill(Color32::from_rgb(233, 240, 251)).truncate().min_size(card);
                    if ui.add_sized(card, button).clicked() {
                        if saving {
                            if me.write_save(slot) {
                                me.say("Сохранено");
                            }
                        } else {
                            me.load_slot(slot);
                        }
                    }
                    if i % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
        });
    }

    fn settings_screen(&mut self, ui: &mut Ui, stage: Rect, back: Screen) {
        self.overlay(ui, stage, "Настройки", |me, ui| {
            me.back_button(ui, back.clone());
            let before = me.settings.clone();
            let s = &mut me.settings;
            egui::Grid::new("settings").num_columns(2).show(ui, |ui| {
                let row = |ui: &mut Ui, label: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>| {
                    let name = ui.label(label);
                    ui.add(egui::Slider::new(value, range).show_value(false)).labelled_by(name.id);
                    ui.end_row();
                };
                row(ui, "Скорость текста", &mut s.text_speed, 0.3..=5.0);
                row(ui, "Пауза авточтения", &mut s.auto_delay, 0.3..=5.0);
                row(ui, "Громкость музыки", &mut s.music, 0.0..=1.0);
                row(ui, "Громкость звуков", &mut s.sfx, 0.0..=1.0);
                row(ui, "Громкость интерфейса", &mut s.ui, 0.0..=1.0);
                ui.checkbox(&mut s.fullscreen, "Полный экран (F11)");
                ui.end_row();
            });
            if me.settings != before {
                if let Err(e) = me.saves.write_settings(&me.settings) {
                    me.say(e);
                }
            }
        });
    }

    fn log_screen(&mut self, ui: &mut Ui, stage: Rect) {
        let lines = self.rt.as_ref().map(|rt| rt.state.backlog.clone()).unwrap_or_default();
        self.overlay(ui, stage, "История диалогов", |me, ui| {
            me.back_button(ui, Screen::Game);
            egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                for l in &lines {
                    ui.horizontal_wrapped(|ui| {
                        if !l.speaker.is_empty() {
                            ui.label(RichText::new(format!("{}:", l.speaker)).color(draw::GOLD));
                        }
                        ui.label(&l.text);
                    });
                }
            });
        });
    }

    fn wardrobe_screen(&mut self, ui: &mut Ui, stage: Rect, slots: &[SlotId], blocking: bool) {
        let Some(rt) = &self.rt else { return };
        let (racks, layers) = (rt.wardrobe(slots), rt.hero_layers());
        let ctx = ui.ctx().clone();
        let doll = Rect::from_min_max(stage.min + vec2(stage.width() * 0.07, stage.height() * 0.2), stage.min + vec2(stage.width() * 0.38, stage.height() * 0.94));
        self.overlay(ui, stage, "Гардероб", |me, ui| {
            let mut equip = None;
            ui.scope_builder(UiBuilder::new().max_rect(Rect::from_min_max(egui::pos2(doll.right() + stage.width() * 0.03, doll.top()), stage.max - vec2(stage.width() * 0.07, stage.height() * 0.06))), |ui| {
                let done = ui.button("Готово").clicked();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for slot in &racks {
                        ui.label(RichText::new(&slot.name).strong());
                        ui.horizontal_wrapped(|ui| {
                            if ui.selectable_label(!slot.items.iter().any(|i| i.equipped), "Ничего").clicked() {
                                equip = Some((slot.id.clone(), None));
                            }
                            for item in &slot.items {
                                if ui.selectable_label(item.equipped, &item.name).clicked() {
                                    equip = Some((slot.id.clone(), Some(item.id.clone())));
                                }
                            }
                        });
                    }
                });
                if done {
                    if blocking {
                        me.screen = Screen::Game;
                        me.advance();
                    } else {
                        me.screen = Screen::Menu;
                        me.refresh(true);
                    }
                }
            });
            if let (Some((slot, item)), Some(rt)) = (equip, &mut me.rt) {
                rt.equip(&slot, item.as_ref());
            }
            let painter = ui.painter_at(doll);
            painter.rect_filled(doll, CornerRadius::same(8), Color32::from_white_alpha(14));
            draw::sprite(&painter, &ctx, &mut me.textures, doll, &layers, [0.5, 1.0, 1.0], 1.0);
        });
    }

    fn draw_toast(&mut self, ui: &mut Ui, stage: Rect, time: f64) {
        let Some((text, until)) = &mut self.toast else { return };
        if *until == 0.0 {
            *until = time + 2.6;
        }
        if time > *until {
            self.toast = None;
            return;
        }
        let painter = ui.painter_at(stage);
        let galley = painter.layout(text.clone(), FontId::proportional(stage.height() * 0.03), Color32::WHITE, stage.width() * 0.8);
        let rect = Rect::from_center_size(egui::pos2(stage.center().x, stage.bottom() - stage.height() * 0.07), galley.size() + vec2(24.0, 14.0));
        painter.rect(rect, CornerRadius::same(8), Color32::from_black_alpha(225), egui::Stroke::new(1.0, draw::ACCENT), egui::StrokeKind::Inside);
        painter.galley(rect.min + vec2(12.0, 7.0), galley, Color32::WHITE);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
    }

    /// Сведения режима отладки для предпросмотра в редакторе.
    pub fn debug_ui(&self, ui: &mut Ui) {
        let Some(rt) = &self.rt else {
            ui.label("Игра не запущена");
            return;
        };
        let st = &rt.state;
        ui.label(format!("Сцена: {} ({})", rt.story.scene(&st.scene).map(|s| s.name.as_str()).unwrap_or("?"), st.scene));
        ui.label(format!("Событие: {}", st.event.as_ref().map(|e| e.as_str()).unwrap_or("—")));
        if let Some(e) = rt.current_event() {
            ui.label(format!("Тип: {}", e.kind.title()));
        }
        ui.label(format!("Музыка: {}", self.audio.current_track().unwrap_or("—")));
        if let Some(Prompt::Error { message }) = self.view.as_ref().map(|v| &v.prompt) {
            ui.colored_label(Color32::LIGHT_RED, format!("Ошибка перехода: {message}"));
        }
        ui.separator();
        ui.strong("Переменные");
        for (k, v) in &st.vars {
            ui.label(format!("{k} = {v}"));
        }
        ui.separator();
        ui.strong("Одежда героя");
        for (slot, item) in &st.outfit {
            ui.label(format!("{slot}: {item}"));
        }
        ui.separator();
        ui.strong("История выборов");
        for c in &st.choices {
            ui.label(format!("• {}", c.text));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vn_core::model::CharId;
    use vn_core::runtime::Music;

    fn sprite(x: f32, fade: f32, move_time: f32) -> Sprite {
        Sprite { id: CharId::from("ann"), layers: vec![], x, y: 1.0, height: 0.9, fade, move_time }
    }

    fn view(background: Option<&str>, sprites: Vec<Sprite>) -> View {
        View { scene: "s".into(), event: None, background: background.map(str::to_string), bg_fade: 1.0, black: false, fade: 0.5, hide_fade: 0.5, sprites, music: None::<Music>, music_fade: 0.0, dialog: Default::default(), prompt: Prompt::Wait { seconds: 1.0 } }
    }

    #[test]
    fn sprite_fades_in_moves_and_fades_out() {
        let mut a = Anim::default();
        a.retarget(&view(None, vec![sprite(0.2, 1.0, 0.0)]), false);
        assert_eq!(a.sprites[0].alpha, 0.0);
        assert!(a.step(0.5));
        assert!((a.sprites[0].alpha - 0.5).abs() < 1e-6);
        a.step(0.6);
        assert_eq!(a.sprites[0].alpha, 1.0);

        a.retarget(&view(None, vec![sprite(0.8, 1.0, 2.0)]), false);
        a.step(1.0);
        assert!((a.sprites[0].cur[0] - 0.5).abs() < 1e-6, "середина пути");
        assert!(a.step(0.5));
        a.step(0.6);
        assert_eq!(a.sprites[0].cur[0], 0.8);

        a.retarget(&view(None, vec![]), false);
        assert_eq!((a.sprites.len(), a.leaving.len()), (0, 1));
        assert!(a.step(0.25));
        a.step(0.3);
        assert!(a.leaving.is_empty());
        assert!(!a.step(0.1), "анимации закончились — перерисовка не нужна");
    }

    #[test]
    fn background_crossfades_and_loading_a_save_snaps() {
        let mut a = Anim::default();
        a.retarget(&view(Some("a.png"), vec![]), false);
        a.step(2.0);
        a.retarget(&view(Some("b.png"), vec![]), false);
        assert_eq!((a.bg_prev.as_deref(), a.bg.as_deref(), a.bg_t), (Some("a.png"), Some("b.png"), 0.0));
        a.step(0.5);
        assert!((a.bg_t - 0.5).abs() < 1e-6);

        let mut dark = view(Some("c.png"), vec![sprite(0.5, 1.0, 1.0)]);
        dark.black = true;
        a.retarget(&dark, true);
        assert_eq!((a.bg_t, a.black, a.sprites[0].alpha), (1.0, 1.0, 1.0));
        assert!(!a.step(0.0));
    }
}
