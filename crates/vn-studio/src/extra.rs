//! Остальные вкладки и окна: предпросмотр, гардероб, концовки, замечания, подтверждения.
use crate::app::{Confirm, Project, Tab, ACCENT};
use crate::ops;
use crate::widgets::{self, pick, pick_opt, row, Lists};
use egui::{Color32, RichText, Sense, Ui};
use vn_core::analysis::endings;
use vn_core::model::*;
use vn_core::validate::Severity;
use vn_core::view::sprite_layers;
use vn_store::export::UpdateKind;
use vn_store::project::{self, AssetKind};
use vn_view::draw;

pub fn preview_ui(ui: &mut Ui, p: &mut Project) {
    let mut debug = ui.data_mut(|d| *d.get_temp_mut_or(egui::Id::unique("preview-debug"), true));
    ui.horizontal(|ui| {
        if ui.button("▶ С начала").clicked() {
            p.start_preview(false);
        }
        if ui.button("▶ С выбранной сцены").clicked() {
            p.start_preview(true);
        }
        if let Some(g) = &mut p.game {
            if ui.button(if g.paused { "▶ Продолжить" } else { "❚❚ Пауза" }).clicked() {
                g.paused = !g.paused;
            }
            if ui.button("⟲ Перезапустить сцену").clicked() {
                let scene = g.runtime().map(|rt| rt.state.scene.clone());
                g.start(scene);
            }
        }
        ui.checkbox(&mut debug, "отладка");
        ui.weak("Правки из редактора попадают в предпросмотр после его перезапуска.");
    });
    ui.data_mut(|d| d.insert_temp(egui::Id::unique("preview-debug"), debug));
    let Some(game) = &mut p.game else {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| ui.label("Нажмите «▶ С начала», чтобы сыграть историю так, как её увидит игрок."));
        return;
    };
    if debug {
        egui::Panel::right("debug").resizable(true).default_size(230.0).show(ui, |ui| {
            ui.strong("Отладка");
            egui::ScrollArea::vertical().show(ui, |ui| {
                game.debug_ui(ui);
            });
        });
    }
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| game.ui(ui));
}

pub fn wardrobe_ui(ui: &mut Ui, p: &mut Project) {
    let lists = Lists::of(&p.story, &p.files);
    let ctx = ui.ctx().clone();
    let mut import_for: Option<SlotId> = None;
    egui::Panel::right("wardrobe-preview").resizable(true).default_size(300.0).show(ui, |ui| {
        ui.strong("Примерка");
        ui.weak("Щёлкните предмет в списке, чтобы надеть или снять его здесь. На игру примерка не влияет.");
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        painter.rect_filled(resp.rect, 6.0, Color32::from_gray(24));
        match p.story.wardrobe.hero.as_ref().and_then(|h| p.story.character(h)) {
            Some(hero) => {
                let expr = hero.expressions.first().map(|e| e.name.clone()).unwrap_or_default();
                let layers = sprite_layers(&p.story, &p.try_on, hero, &expr);
                if draw::sprite(&painter, &ctx, &mut p.textures, resp.rect, &layers, [0.5, 0.98, 0.94], 1.0).is_none() {
                    draw::placeholder(&painter, resp.rect, &hero.name, hero.color, 0.5, 0.98, 0.94);
                }
            }
            None => drop(painter.text(resp.rect.center(), egui::Align2::CENTER_CENTER, "Главный герой не выбран", egui::FontId::proportional(14.0), Color32::GRAY)),
        }
    });
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let w = &mut p.story.wardrobe;
        row(ui, "Главный герой", |ui| pick_opt(ui, "hero", &mut w.hero, &lists.chars, "— не выбран —"));
        ui.checkbox(&mut w.anytime, "игрок может открыть гардероб из меню в любой момент");
        ui.weak("Одежда — это слои поверх спрайта героя. Все изображения слоёв должны быть одного размера с телом героя (прозрачный PNG). Порядок слотов — порядок слоёв снизу вверх.");
        ui.separator();
        let files = lists.assets(AssetKind::Clothing);
        let mut slot_action: Option<(usize, &str)> = None;
        let count = w.slots.len();
        for n in 0..count {
            let slot_id = w.slots[n].id.clone();
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut w.slots[n].name).desired_width(130.0).font(egui::TextStyle::Button));
                    ui.checkbox(&mut w.slots[n].behind, "под телом").on_hover_text("Слой рисуется позади тела героя (например, задняя часть волос)");
                    for (label, key) in [("▲", "up"), ("▼", "down"), ("✖", "del")] {
                        if ui.small_button(label).clicked() {
                            slot_action = Some((n, key));
                        }
                    }
                });
                let mut remove = None;
                for item in w.items.iter_mut().filter(|i| i.slot == slot_id) {
                    ui.horizontal(|ui| {
                        let worn = p.try_on.get(&slot_id) == Some(&item.id);
                        if ui.selectable_label(worn, "●").on_hover_text("Примерить").clicked() {
                            if worn {
                                p.try_on.remove(&slot_id);
                            } else {
                                p.try_on.insert(slot_id.clone(), item.id.clone());
                            }
                        }
                        ui.add(egui::TextEdit::singleline(&mut item.name).desired_width(120.0).hint_text("название"));
                        pick(ui, (&item.id, "img"), &mut item.image, &files, "— файл —");
                        ui.checkbox(&mut item.unlocked, "доступен сразу").on_hover_text("Иначе предмет открывается событием «Открыть предмет»");
                        if ui.small_button("✖").clicked() {
                            remove = Some(item.id.clone());
                        }
                    });
                }
                if let Some(id) = remove {
                    w.items.retain(|i| i.id != id);
                    p.try_on.retain(|_, i| *i != id);
                }
                ui.horizontal(|ui| {
                    if ui.small_button("+ Предмет").clicked() {
                        w.items.push(Item { id: ItemId::new("it"), slot: slot_id.clone(), name: "Новый предмет".into(), image: String::new(), unlocked: true });
                    }
                    if ui.small_button("Импорт изображений…").on_hover_text("Каждый файл станет предметом этого слота").clicked() {
                        import_for = Some(slot_id.clone());
                    }
                });
            });
        }
        match slot_action {
            Some((n, "up")) if n > 0 => w.slots.swap(n, n - 1),
            Some((n, "down")) if n + 1 < count => w.slots.swap(n, n + 1),
            Some((n, "del")) => {
                let id = w.slots.remove(n).id;
                w.items.retain(|i| i.slot != id);
                p.try_on.remove(&id);
            }
            _ => {}
        }
        ui.horizontal(|ui| {
            if ui.button("+ Слот").clicked() {
                w.slots.push(Slot { id: SlotId::new("slot"), name: "Новый слот".into(), behind: false });
            }
            if w.slots.is_empty() && ui.button("Создать стандартные слоты").clicked() {
                w.slots = [("Причёска (сзади)", true), ("Низ", false), ("Обувь", false), ("Верх", false), ("Причёска", false), ("Аксессуары", false)].into_iter().map(|(name, behind)| Slot { id: SlotId::new("slot"), name: name.into(), behind }).collect();
            }
        });
    });
    if let Some(slot) = import_for {
        if let Some(paths) = rfd::FileDialog::new().add_filter("Изображения", AssetKind::Clothing.extensions()).pick_files() {
            for image in p.import(AssetKind::Clothing, &paths) {
                let name = ops::asset_name(&image).rsplit_once('.').map(|(n, _)| n.to_string()).unwrap_or_default();
                p.story.wardrobe.items.push(Item { id: ItemId::new("it"), slot: slot.clone(), name, image, unlocked: true });
            }
        }
    }
}

pub fn endings_ui(ui: &mut Ui, p: &mut Project) {
    let list = endings(&p.story);
    let name = |id: &SceneId| p.story.scene(id).map(|s| s.name.clone()).unwrap_or_else(|| id.to_string());
    let reachable = list.iter().filter(|e| e.reachable).count();
    ui.heading(format!("Концовок: {} (достижимых: {reachable})", list.len()));
    let live = p.story.meta.start_scene.as_ref().map(|s| vn_core::analysis::reachable(&p.story, s)).unwrap_or_default();
    let dead: Vec<String> = p.story.scenes.iter().filter(|s| !live.contains(&s.id)).map(|s| s.name.clone()).collect();
    if !dead.is_empty() {
        ui.colored_label(Color32::from_rgb(255, 190, 90), format!("Сцены, в которые невозможно попасть от начала: {}", dead.join(", ")));
    }
    let mut open = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if list.is_empty() {
            ui.label("В истории пока нет событий «Концовка». Добавьте его последним событием финальной сцены.");
        }
        for e in &list {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("⚑ {}", e.title)).strong().color(draw::GOLD));
                    ui.weak(format!("сцена «{}»", name(&e.scene)));
                    if !e.reachable {
                        ui.colored_label(Color32::LIGHT_RED, "недостижима");
                    }
                    if ui.small_button("Открыть").clicked() {
                        open = Some((e.scene.clone(), e.event.clone()));
                    }
                });
                ui.label(format!("Путь проходит через сцены: {}", if e.leading.is_empty() { "—".to_string() } else { e.leading.iter().map(&name).collect::<Vec<_>>().join(" → ") }));
                if e.decisive.is_empty() {
                    ui.weak("От выборов игрока не зависит.");
                } else {
                    ui.label("Чтобы прийти к ней, нужно выбрать:");
                    for d in &e.decisive {
                        ui.label(format!("   • в сцене «{}»: {}", name(&d.scene), d.option));
                    }
                }
            });
        }
    });
    if let Some((scene, event)) = open {
        p.select(Some(scene), Some(event));
        p.tab = Tab::Stage;
    }
}

pub fn issues_ui(ui: &mut Ui, p: &mut Project) {
    let Some(issues) = p.issues.clone() else { return };
    let errors = issues.iter().filter(|i| i.severity == Severity::Error).count();
    let mut close = false;
    ui.horizontal(|ui| {
        ui.strong(if issues.is_empty() { "Проверка: замечаний нет ✔".to_string() } else { format!("Проверка: ошибок — {errors}, предупреждений — {}", issues.len() - errors) });
        ui.weak("Ошибки блокируют сборку, предупреждения — нет. Щёлкните строку, чтобы перейти к месту.");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| close = ui.small_button("✖ Скрыть").clicked());
    });
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for issue in &issues {
            let (mark, color) = if issue.severity == Severity::Error { ("✖", Color32::LIGHT_RED) } else { ("⚠", Color32::from_rgb(255, 200, 100)) };
            let place = issue.scene.as_ref().and_then(|s| p.story.scene(s)).map(|s| format!("[{}] ", s.name)).unwrap_or_default();
            if ui.add(egui::Label::new(RichText::new(format!("{mark} {place}{}", issue.message)).color(color)).sense(Sense::click())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && issue.scene.is_some() {
                p.select(issue.scene.clone(), issue.event.clone());
                p.tab = Tab::Stage;
            }
        }
    });
    if close {
        p.issues = None;
    }
}

fn modal<R>(ctx: &egui::Context, title: &str, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
        ui.set_max_width(480.0);
        add(ui)
    })?.inner
}

pub fn confirm_window(ctx: &egui::Context, p: &mut Project) {
    let Some(confirm) = &mut p.confirm else { return };
    let lists = Lists::of(&p.story, &p.files);
    let (title, yes) = match confirm {
        Confirm::Autosave(_) => ("Несохранённые изменения", "Восстановить"),
        _ => ("Подтвердите удаление", "Удалить"),
    };
    let answer = modal(ctx, title, |ui| {
        match confirm {
            Confirm::Scene { id, redirect } => {
                let links = p.story.edges().iter().filter(|e| &e.to == id && &e.from != id).count();
                ui.label(format!("Удалить сцену «{}»?", lists.scenes.iter().find(|(s, _)| s == id).map(|(_, n)| n.as_str()).unwrap_or("?")));
                if links > 0 {
                    ui.colored_label(Color32::from_rgb(255, 200, 100), format!("На неё ведут переходы: {links}. После удаления проверка покажет их как ошибки."));
                }
                ui.label("Игроки, сохранившиеся в этой сцене, после обновления продолжат со сцены:");
                let others: Vec<(SceneId, String)> = lists.scenes.iter().filter(|(s, _)| s != id).cloned().collect();
                pick_opt(ui, "redirect", redirect, &others, "по общему правилу проекта");
            }
            Confirm::Asset(path) => drop(ui.label(format!("Удалить файл «{}» с диска? Ссылки на него в истории будут очищены. Это действие нельзя отменить.", ops::asset_name(path)))),
            Confirm::Character(id) => drop(ui.label(format!("Удалить персонажа «{}»? События с ним останутся и будут показаны проверкой как ошибки.", lists.chars.iter().find(|(c, _)| c == id).map(|(_, n)| n.as_str()).unwrap_or("?")))),
            Confirm::Chapter(id) => drop(ui.label(format!("Удалить главу «{}»? Её сцены останутся и перейдут в раздел «Без главы».", lists.chapters.iter().find(|(c, _)| c == id).map(|(_, n)| n.as_str()).unwrap_or("?")))),
            Confirm::Autosave(_) => drop(ui.label("Редактор был закрыт без сохранения, но успел сделать автосохранение. Восстановить эти изменения?")),
        }
        ui.horizontal(|ui| if ui.button(yes).clicked() { Some(true) } else if ui.button(if matches!(confirm, Confirm::Autosave(_)) { "Отбросить" } else { "Отмена" }).clicked() { Some(false) } else { None }).inner
    })
    .flatten();
    let Some(yes) = answer else { return };
    match (p.confirm.take().expect("окно открыто"), yes) {
        (Confirm::Scene { id, redirect }, true) => p.story.remove_scene(&id, redirect),
        (Confirm::Asset(path), true) => match std::fs::remove_file(p.dir.join(&path)) {
            Ok(()) => {
                ops::forget_asset(&mut p.story, &path);
                p.rescan();
            }
            Err(e) => p.status = format!("Файл не удалён: {e}"),
        },
        (Confirm::Character(id), true) => {
            p.story.characters.retain(|c| c.id != id);
            if p.story.wardrobe.hero.as_ref() == Some(&id) {
                p.story.wardrobe.hero = None;
            }
        }
        (Confirm::Chapter(id), true) => {
            p.story.chapters.retain(|c| c.id != id);
            for s in &mut p.story.scenes {
                if s.chapter.as_ref() == Some(&id) {
                    s.chapter = None;
                }
            }
        }
        (Confirm::Autosave(story), true) => {
            p.story = *story;
            p.status = "Изменения восстановлены из автосохранения. Сохраните проект (Ctrl+S).".into();
        }
        (Confirm::Autosave(_), false) => project::clear_autosave(&p.dir),
        _ => {}
    }
}

pub fn built_window(ctx: &egui::Context, p: &mut Project) {
    let Some(b) = &p.built else { return };
    let mut close = false;
    let mut serve = false;
    modal(ctx, "Готово", |ui| {
        ui.label(&b.message);
        ui.add(egui::Label::new(RichText::new(b.path.display().to_string()).monospace()).wrap());
        ui.horizontal(|ui| {
            if ui.button("Открыть папку").clicked() {
                widgets::reveal(if b.path.is_dir() { &b.path } else { b.path.parent().unwrap_or(&b.path) });
            }
            if b.web && p.lan.is_none() && ui.button("Проверить на телефоне (Wi-Fi)").clicked() {
                serve = true;
            }
            close = ui.button("Закрыть").clicked();
        });
        if let Some(server) = &p.lan {
            ui.separator();
            ui.label("Откройте на телефоне, подключённом к той же сети Wi-Fi, адрес:");
            let url = server.url();
            ui.horizontal(|ui| {
                ui.label(RichText::new(&url).size(22.0).color(ACCENT).monospace());
                if ui.small_button("Копировать").clicked() {
                    ui.ctx().copy_text(url.clone());
                }
            });
            ui.weak("Адрес работает, пока открыто это окно. Если страница не открывается, разрешите редактору доступ к сети в брандмауэре Windows. Установка на главный экран и работа без интернета доступны после размещения на хостинге с https.");
        }
    });
    if serve {
        match crate::lanserve::Server::start(b.path.clone()) {
            Ok(s) => p.lan = Some(s),
            Err(e) => p.status = e,
        }
    }
    if close {
        p.built = None;
        p.lan = None;
    }
}

pub fn notes_window(ctx: &egui::Context, p: &mut Project) {
    if p.notes.is_empty() {
        return;
    }
    let close = modal(ctx, "Проект открыт с замечаниями", |ui| {
        ui.label("Часть файлов проекта не удалось прочитать. Остальное открыто; повреждённые файлы не удалялись.");
        for n in &p.notes {
            ui.label(format!("• {n}"));
        }
        ui.button("Понятно").clicked()
    });
    if close == Some(true) {
        p.notes.clear();
    }
}

/// Окно выпуска обновления. Возвращает версию и состав пакета, когда автор подтвердил.
pub fn release_window(ctx: &egui::Context, p: &mut Project) -> Option<(String, UpdateKind)> {
    let next = p.story.meta.story_version + 1;
    let (version, kind) = p.release.as_mut()?;
    let answer = modal(ctx, "Выпуск обновления истории", |ui| {
        ui.label("Будет создан файл обновления (.vnpak). Игрок кладёт его в папку игры — история обновляется, сохранения остаются и продолжаются с прежнего места.");
        ui.radio_value(kind, UpdateKind::Full, "Полное: сценарий и ресурсы");
        ui.radio_value(kind, UpdateKind::Story, "Только сценарий — маленький файл, если новых картинок и музыки нет");
        ui.radio_value(kind, UpdateKind::Assets, "Только ресурсы — заменить картинки и звук, сценарий не трогать");
        if *kind == UpdateKind::Assets {
            ui.weak("Номер выпуска не меняется: пакет ресурсов ставится поверх текущего сценария.");
        } else {
            row(ui, "Версия", |ui| ui.add(egui::TextEdit::singleline(version).desired_width(80.0)));
            ui.weak(format!("Это будет выпуск №{next}."));
        }
        ui.horizontal(|ui| if ui.button("Выпустить").clicked() { Some(true) } else if ui.button("Отмена").clicked() { Some(false) } else { None }).inner
    })
    .flatten()?;
    let chosen = p.release.take()?;
    answer.then_some(chosen)
}
