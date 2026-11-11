//! Левая панель: сцены и главы, персонажи, ресурсы, переменные, настройки проекта.
use crate::app::{Confirm, Dragged, LeftTab, Project, Tab, ACCENT};
use crate::ops;
use crate::widgets::{self, pick, pick_opt, row, Lists};
use egui::{RichText, Sense, Ui};
use vn_core::model::*;
use vn_core::templates::{duplicate_chapter, duplicate_scene};
use vn_store::project::AssetKind;

fn matches(name: &str, search: &str) -> bool {
    search.trim().is_empty() || name.to_lowercase().contains(&search.trim().to_lowercase())
}

fn scenes_tab(ui: &mut Ui, p: &mut Project) {
    ui.horizontal(|ui| {
        if ui.button("+ Сцена").clicked() {
            let mut scene = Scene::new(&format!("Сцена {}", p.story.scenes.len() + 1));
            scene.chapter = p.current_scene().and_then(|s| s.chapter.clone());
            p.select(Some(scene.id.clone()), None);
            p.story.scenes.push(scene);
            ops::auto_layout(&mut p.story, false);
        }
        if ui.button("+ Глава").clicked() {
            p.story.chapters.push(Chapter { id: ChapterId::new("chp"), name: format!("Глава {}", p.story.chapters.len() + 1) });
        }
    });
    ui.add(egui::TextEdit::singleline(&mut p.search).desired_width(f32::INFINITY).hint_text("Поиск сцен"));
    let mut groups: Vec<(Option<ChapterId>, String)> = p.story.chapters.iter().map(|c| (Some(c.id.clone()), c.name.clone())).collect();
    let known: Vec<ChapterId> = p.story.chapters.iter().map(|c| c.id.clone()).collect();
    if groups.is_empty() || p.story.scenes.iter().any(|s| s.chapter.as_ref().is_none_or(|c| !known.contains(c))) {
        groups.push((None, if groups.is_empty() { "Сцены".into() } else { "Без главы".into() }));
    }
    let start = p.story.meta.start_scene.clone();
    let mut action: Option<(&str, SceneId)> = None;
    let mut chapter_action: Option<(&str, ChapterId)> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (chapter, title) in &groups {
            ui.horizontal(|ui| {
                match chapter.as_ref().and_then(|id| p.story.chapters.iter_mut().find(|c| &c.id == id)) {
                    Some(c) => {
                        ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(150.0).frame(egui::Frame::NONE).font(egui::TextStyle::Button).text_color(ACCENT));
                        let id = c.id.clone();
                        ui.menu_button("⋯", |ui| {
                            for (name, key) in [("Дублировать главу со сценами", "dup"), ("Удалить главу…", "del")] {
                                if ui.button(name).clicked() {
                                    chapter_action = Some((key, id.clone()));
                                    ui.close();
                                }
                            }
                        });
                    }
                    None => drop(ui.label(RichText::new(title).color(ACCENT))),
                };
            });
            let in_group = |s: &Scene| match chapter {
                Some(c) => s.chapter.as_ref() == Some(c),
                None => s.chapter.as_ref().is_none_or(|c| !known.contains(c)),
            };
            let rows: Vec<(SceneId, String)> = p.story.scenes.iter().filter(|s| in_group(s) && matches(&s.name, &p.search)).map(|s| (s.id.clone(), s.name.clone())).collect();
            for (id, name) in rows {
                let label = if start.as_ref() == Some(&id) { format!("▶ {name}") } else { format!("   {name}") };
                let resp = ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| ui.add(egui::Button::selectable(p.scene.as_ref() == Some(&id), label).truncate())).inner;
                if resp.clicked() {
                    p.select(Some(id.clone()), None);
                    if p.tab != Tab::Graph {
                        p.tab = Tab::Stage;
                    }
                }
                resp.context_menu(|ui| {
                    for (name, key) in [("Дублировать", "dup"), ("Сделать начальной", "start"), ("Удалить…", "del")] {
                        if ui.button(name).clicked() {
                            action = Some((key, id.clone()));
                            ui.close();
                        }
                    }
                });
            }
            ui.add_space(4.0);
        }
    });
    match action {
        Some(("dup", id)) => {
            if let Some(new) = duplicate_scene(&mut p.story, &id) {
                ops::auto_layout(&mut p.story, false);
                p.select(Some(new), None);
            }
        }
        Some(("start", id)) => p.story.meta.start_scene = Some(id),
        Some((_, id)) => p.confirm = Some(Confirm::Scene { id, redirect: None }),
        None => {}
    }
    match chapter_action {
        Some(("dup", id)) => {
            duplicate_chapter(&mut p.story, &id);
            ops::auto_layout(&mut p.story, false);
        }
        Some((_, id)) => p.confirm = Some(Confirm::Chapter(id)),
        None => {}
    }
}

fn characters_tab(ui: &mut Ui, p: &mut Project) {
    if ui.button("+ Персонаж").clicked() {
        let c = Character::new(&format!("Персонаж {}", p.story.characters.len() + 1));
        p.character = Some(c.id.clone());
        p.story.characters.push(c);
    }
    ui.weak("Перетащите персонажа на сцену, чтобы он появился.");
    let rows: Vec<(CharId, String, [u8; 3])> = p.story.characters.iter().map(|c| (c.id.clone(), c.name.clone(), c.color)).collect();
    for (id, name, color) in rows {
        let text = RichText::new(format!("● {name}")).color(egui::Color32::from_rgb(color[0], color[1], color[2]));
        let resp = ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| ui.add(egui::Button::selectable(p.character.as_ref() == Some(&id), text))).inner.interact(Sense::drag());
        resp.dnd_set_drag_payload(Dragged::Character(id.clone()));
        if resp.clicked() {
            p.character = Some(id);
        }
    }
    ui.separator();
    let Some(cid) = p.character.clone() else { return };
    let lists = Lists::of(&p.story, &p.files);
    let mut is_hero = p.story.wardrobe.hero.as_ref() == Some(&cid);
    let mut import = false;
    let mut delete = false;
    let Some(c) = p.story.characters.iter_mut().find(|c| c.id == cid) else { return };
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        row(ui, "Имя", |ui| {
            ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(120.0));
            ui.color_edit_button_srgb(&mut c.color).on_hover_text("Цвет имени в диалоге");
        });
        ui.add(egui::TextEdit::multiline(&mut c.description).desired_rows(2).desired_width(f32::INFINITY).hint_text("Описание (для автора и для ИИ)"));
        row(ui, "Рост на экране", |ui| ui.add(egui::Slider::new(&mut c.height, 0.3..=1.5).fixed_decimals(2)).on_hover_text("Высота спрайта в долях высоты экрана"));
        ui.checkbox(&mut is_hero, "главный герой (его одевает игрок)");
        ui.add_space(4.0);
        ui.strong("Выражения лица / варианты спрайта");
        let files = lists.assets(AssetKind::Character);
        let mut remove = None;
        for (n, e) in c.expressions.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut e.name).desired_width(70.0).hint_text("название"));
                pick(ui, (&cid, n, "img"), &mut e.image, &files, "— файл —");
                if ui.small_button("✖").clicked() {
                    remove = Some(n);
                }
            });
        }
        if let Some(n) = remove {
            c.expressions.remove(n);
        }
        ui.horizontal(|ui| {
            if ui.button("+ Выражение").clicked() {
                c.expressions.push(Expression { name: format!("вид {}", c.expressions.len() + 1), image: String::new() });
            }
            import = ui.button("Импорт изображений…").on_hover_text("Каждый файл станет отдельным выражением").clicked();
        });
        ui.add_space(8.0);
        delete = ui.button("Удалить персонажа…").clicked();
        ui.weak(format!("ID: {cid}"));
    });
    if is_hero != (p.story.wardrobe.hero.as_ref() == Some(&cid)) {
        p.story.wardrobe.hero = is_hero.then(|| cid.clone());
    }
    if import {
        if let Some(paths) = rfd::FileDialog::new().add_filter("Изображения", AssetKind::Character.extensions()).pick_files() {
            let added = p.import(AssetKind::Character, &paths);
            if let Some(c) = p.story.characters.iter_mut().find(|c| c.id == cid) {
                c.expressions.extend(added.into_iter().map(|image| Expression { name: ops::asset_name(&image).rsplit_once('.').map(|(n, _)| n.to_string()).unwrap_or_default(), image }));
            }
        }
    }
    if delete {
        p.confirm = Some(Confirm::Character(cid));
    }
}

fn assets_tab(ui: &mut Ui, p: &mut Project) {
    ui.horizontal(|ui| {
        if ui.button("⟳ Обновить список").on_hover_text("Перечитать папку assets (если файлы меняли вне редактора)").clicked() {
            p.rescan();
        }
        if ui.button("Папка").on_hover_text("Открыть папку ресурсов").clicked() {
            widgets::reveal(&p.dir.join("assets"));
        }
    });
    ui.add(egui::TextEdit::singleline(&mut p.search).desired_width(f32::INFINITY).hint_text("Поиск ресурсов"));
    ui.weak("Файлы можно перетащить в окно редактора. Фон, музыку и звук — перетащить из списка на сцену.");
    let used = p.story.assets_used();
    let mut import = None;
    let mut play = None;
    let ctx = ui.ctx().clone();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for kind in AssetKind::ALL {
            let prefix = format!("{}/", kind.folder());
            let files: Vec<String> = p.files.iter().filter(|f| f.starts_with(&prefix) && matches(f, &p.search)).cloned().collect();
            egui::CollapsingHeader::new(format!("{} ({})", kind.title(), files.len())).default_open(true).show(ui, |ui| {
                if ui.small_button("+ Импорт…").clicked() {
                    import = Some(kind);
                }
                for f in files {
                    ui.horizontal(|ui| {
                        if !kind.is_audio() {
                            let (rect, _) = ui.allocate_exact_size(egui::vec2(34.0, 22.0), Sense::hover());
                            if let Some(tex) = p.textures.get(&ctx, &f) {
                                ui.painter().image(tex.id(), rect, vn_view::draw::cover_uv(tex.size_vec2(), rect), egui::Color32::WHITE);
                            }
                        } else if ui.small_button("▶").on_hover_text("Прослушать").clicked() {
                            play = Some(f.clone());
                        }
                        let name = &f[prefix.len()..];
                        let text = if used.contains(&f) { RichText::new(name) } else { RichText::new(name).weak().italics() };
                        let resp = ui.add(egui::Label::new(text).truncate().sense(Sense::drag())).on_hover_text(if used.contains(&f) { "Используется в истории" } else { "Не используется в истории" });
                        resp.dnd_set_drag_payload(Dragged::Asset(f.clone()));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✖").on_hover_text("Удалить файл").clicked() {
                                p.confirm = Some(Confirm::Asset(f.clone()));
                            }
                        });
                    });
                }
            });
        }
        if p.audio.update(0.0, 1.0) && ui.button("■ Остановить звук").clicked() {
            p.audio.stop_all();
        }
    });
    if let Some(f) = play {
        p.audio.stop_all();
        match std::fs::read(p.dir.join(&f)) {
            Ok(bytes) => p.audio.play_sound(bytes, 1.0),
            Err(e) => p.status = format!("Файл не читается: {e}"),
        }
    }
    if let Some(kind) = import {
        p.import_kind = kind;
        if let Some(paths) = rfd::FileDialog::new().add_filter(kind.title(), kind.extensions()).pick_files() {
            p.import(kind, &paths);
        }
    }
}

fn variables_tab(ui: &mut Ui, p: &mut Project) {
    if ui.button("+ Переменная").clicked() {
        p.story.variables.push(VarDef { name: format!("переменная_{}", p.story.variables.len() + 1), initial: Value::Int(0), description: String::new() });
    }
    ui.weak("Переменные хранят последствия выборов: доверие, деньги, найден ли предмет. В тексте подставляются как {имя}.");
    let mut remove = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (n, v) in p.story.variables.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut v.name).desired_width(110.0).hint_text("имя"));
                ui.label("=");
                widgets::value_edit(ui, &mut v.initial);
                if ui.small_button("✖").clicked() {
                    remove = Some(n);
                }
            });
            ui.add(egui::TextEdit::singleline(&mut v.description).desired_width(f32::INFINITY).hint_text("что означает"));
            ui.separator();
        }
    });
    if let Some(n) = remove {
        p.story.variables.remove(n);
    }
}

fn project_tab(ui: &mut Ui, p: &mut Project) {
    let lists = Lists::of(&p.story, &p.files);
    let m = &mut p.story.meta;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        row(ui, "Название", |ui| ui.text_edit_singleline(&mut m.name));
        row(ui, "Автор", |ui| ui.text_edit_singleline(&mut m.author));
        ui.add(egui::TextEdit::multiline(&mut m.description).desired_rows(2).desired_width(f32::INFINITY).hint_text("Описание"));
        row(ui, "Версия", |ui| {
            ui.add(egui::TextEdit::singleline(&mut m.version).desired_width(60.0));
            ui.weak(format!("выпуск {}", m.story_version));
        });
        row(ui, "Разрешение", |ui| {
            ui.add(egui::DragValue::new(&mut m.resolution[0]).range(320..=7680));
            ui.label("×");
            ui.add(egui::DragValue::new(&mut m.resolution[1]).range(180..=4320));
        });
        ui.checkbox(&mut m.fullscreen, "запускать игру на весь экран");
        ui.separator();
        ui.strong("Текст в игре");
        row(ui, "Размер", |ui| ui.add(egui::Slider::new(&mut m.text.size, 0.02..=0.08).fixed_decimals(3)).on_hover_text("Высота шрифта в долях высоты экрана"));
        row(ui, "Цвет", |ui| ui.color_edit_button_srgb(&mut m.text.color));
        row(ui, "Скорость", |ui| ui.add(egui::Slider::new(&mut m.text.speed, 5.0..=200.0).suffix(" симв/с").fixed_decimals(0)));
        ui.separator();
        ui.strong("Обновления и сохранения игроков");
        row(ui, "Начальная сцена", |ui| pick_opt(ui, "meta-start", &mut m.start_scene, &lists.scenes, "⚠ не выбрана"));
        ui.label("Если сцена, в которой сохранился игрок, удалена и для неё не задана замена:");
        pick_opt(ui, "meta-fallback", &mut m.fallback_scene, &lists.scenes, "сообщить, что сохранение несовместимо");
        if !p.story.redirects.is_empty() {
            ui.add_space(4.0);
            ui.label("Замены удалённых сцен:");
            let mut drop_key = None;
            for (from, to) in &p.story.redirects {
                ui.horizontal(|ui| {
                    ui.weak(format!("{from} → {}", lists.scenes.iter().find(|(id, _)| id == to).map(|(_, n)| n.as_str()).unwrap_or("?")));
                    if ui.small_button("✖").clicked() {
                        drop_key = Some(from.clone());
                    }
                });
            }
            if let Some(k) = drop_key {
                p.story.redirects.remove(&k);
            }
        }
        ui.separator();
        ui.weak(format!("Папка проекта: {}", p.dir.display()));
        if ui.button("Открыть папку проекта").clicked() {
            widgets::reveal(&p.dir);
        }
    });
}

pub fn ui(ui: &mut Ui, p: &mut Project) {
    ui.horizontal_wrapped(|ui| {
        for (tab, name) in [(LeftTab::Scenes, "Сцены"), (LeftTab::Characters, "Персонажи"), (LeftTab::Assets, "Ресурсы"), (LeftTab::Variables, "Переменные"), (LeftTab::Project, "Проект")] {
            if ui.selectable_label(p.left == tab, name).clicked() {
                p.left = tab;
                p.search.clear();
            }
        }
    });
    ui.separator();
    match p.left {
        LeftTab::Scenes => scenes_tab(ui, p),
        LeftTab::Characters => characters_tab(ui, p),
        LeftTab::Assets => assets_tab(ui, p),
        LeftTab::Variables => variables_tab(ui, p),
        LeftTab::Project => project_tab(ui, p),
    }
}
