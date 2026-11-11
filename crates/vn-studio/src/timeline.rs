//! Таймлайн: последовательность событий выбранной сцены.
use crate::app::{Project, ACCENT};
use crate::ops;
use crate::widgets::{self, Lists};
use egui::{Color32, RichText, Ui};
use vn_core::model::*;
use vn_core::templates::Template;

fn color(kind: &EventKind) -> Color32 {
    match kind {
        EventKind::Say { .. } | EventKind::DialogBox { .. } => Color32::from_rgb(120, 170, 255),
        EventKind::Background { .. } | EventKind::Fade { .. } => Color32::from_rgb(120, 200, 140),
        EventKind::ShowCharacter { .. } | EventKind::HideCharacter { .. } | EventKind::MoveCharacter { .. } | EventKind::Expression { .. } => Color32::from_rgb(90, 210, 200),
        EventKind::PlayMusic { .. } | EventKind::StopMusic { .. } | EventKind::MusicVolume { .. } | EventKind::Sound { .. } => Color32::from_rgb(230, 170, 90),
        EventKind::Choice { .. } | EventKind::Goto { .. } | EventKind::Branch { .. } | EventKind::End { .. } => Color32::from_rgb(200, 120, 255),
        EventKind::SetVar(_) => Color32::from_rgb(240, 110, 160),
        EventKind::SetOutfit { .. } | EventKind::UnlockItem { .. } | EventKind::OpenWardrobe { .. } => Color32::from_rgb(240, 220, 110),
        EventKind::Wait { .. } => Color32::GRAY,
    }
}

pub fn delete_selected(p: &mut Project) {
    let (Some(sid), Some(i)) = (p.scene.clone(), p.event_index()) else { return };
    let Some(scene) = p.story.scene_mut(&sid) else { return };
    scene.events.remove(i);
    p.event = scene.events.get(i.min(scene.events.len().saturating_sub(1))).map(|e| e.id.clone());
}

pub fn duplicate_selected(p: &mut Project) {
    let (Some(sid), Some(i)) = (p.scene.clone(), p.event_index()) else { return };
    let Some(scene) = p.story.scene_mut(&sid) else { return };
    let copy = ops::clone_event(&scene.events[i]);
    p.event = ops::insert_events(scene, Some(i), vec![copy]);
}

/// Добавить события после выбранного (или в конец сцены) и выбрать первое из них.
pub fn add_events(p: &mut Project, events: Vec<Event>) {
    let after = p.event_index();
    let Some(scene) = p.scene.clone().and_then(|id| p.story.scene_mut(&id)) else { return };
    p.event = ops::insert_events(scene, after, events);
}

fn scene_header(ui: &mut Ui, p: &mut Project, lists: &Lists) {
    let Some(sid) = p.scene.clone() else { return };
    let is_start = p.story.meta.start_scene.as_ref() == Some(&sid);
    let mut make_start = false;
    let Some(scene) = p.story.scene_mut(&sid) else { return };
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut scene.name).desired_width(170.0).hint_text("Название сцены").font(egui::TextStyle::Heading));
        if is_start {
            ui.label(RichText::new("▶ начало истории").color(ACCENT)).on_hover_text("С этой сцены начинается игра");
        } else if ui.small_button("Сделать начальной").clicked() {
            make_start = true;
        }
    });
    ui.add(egui::TextEdit::singleline(&mut scene.description).desired_width(f32::INFINITY).hint_text("Заметка для автора (игрок её не видит)"));
    if !lists.chapters.is_empty() {
        widgets::row(ui, "Глава", |ui| widgets::pick_opt(ui, "scene-chapter", &mut scene.chapter, &lists.chapters, "без главы"));
    }
    ui.weak(format!("ID: {sid}"));
    if make_start {
        p.story.meta.start_scene = Some(sid);
    }
}

fn add_menu(ui: &mut Ui, p: &mut Project) {
    let Some(sid) = p.scene.clone() else { return };
    ui.horizontal(|ui| {
        ui.menu_button(RichText::new("+ Событие").color(Color32::WHITE), |ui| {
            for kind in ops::palette(&p.story, &sid) {
                if ui.button(ops::palette_title(&kind)).clicked() {
                    add_events(p, vec![Event::new(kind)]);
                    ui.close();
                }
            }
        });
        ui.menu_button("Шаблон", |ui| {
            for t in Template::ALL {
                if ui.button(t.name()).clicked() {
                    let events = t.events(&p.story);
                    add_events(p, events);
                    ui.close();
                }
            }
        });
        if ui.button("▶ Отсюда").on_hover_text("Запустить предпросмотр с этой сцены").clicked() {
            p.start_preview(true);
        }
    });
}

pub fn ui(ui: &mut Ui, p: &mut Project) {
    let lists = Lists::of(&p.story, &p.files);
    scene_header(ui, p, &lists);
    ui.separator();
    add_menu(ui, p);
    ui.separator();
    let Some(sid) = p.scene.clone() else { return };
    let Some(scene) = p.story.scene(&sid) else { return };
    let rows: Vec<(EventId, String, Color32, f32)> = scene.events.iter().map(|e| (e.id.clone(), ops::summary(&p.story, &e.kind), color(&e.kind), e.delay)).collect();
    if rows.is_empty() {
        ui.weak("В сцене пока нет событий. Нажмите «+ Событие» или выберите шаблон. Фон и персонажей можно перетащить из левой панели прямо на сцену.");
    }
    let mut action: Option<(usize, &str)> = None;
    let mut moved: Option<(usize, usize)> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (i, (id, text, tint, delay)) in rows.iter().enumerate() {
            let selected = p.event.as_ref() == Some(id);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click_and_drag());
            let resp = resp.on_hover_text(text.as_str());
            if ui.is_rect_visible(rect) {
                let fill = if selected {
                    ui.visuals().selection.bg_fill
                } else if resp.hovered() {
                    ui.visuals().widgets.hovered.weak_bg_fill
                } else {
                    Color32::from_gray(32)
                };
                let painter = ui.painter().with_clip_rect(rect);
                painter.rect_filled(rect.shrink2(egui::vec2(0.0, 1.0)), 4.0, fill);
                painter.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(26.0, 3.0), egui::vec2(4.0, 18.0)), 2.0, *tint);
                painter.text(rect.left_center() + egui::vec2(20.0, 0.0), egui::Align2::RIGHT_CENTER, (i + 1).to_string(), egui::FontId::monospace(11.0), Color32::GRAY);
                let label = if *delay > 0.0 { format!("(+{delay:.1} с) {text}") } else { text.clone() };
                painter.text(rect.left_center() + egui::vec2(38.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(13.5), if selected { Color32::WHITE } else { ui.visuals().text_color() });
            }
            resp.dnd_set_drag_payload(i);
            if resp.dnd_hover_payload::<usize>().is_some_and(|from| *from != i) {
                ui.painter().hline(rect.x_range(), rect.top(), egui::Stroke::new(2.0, ACCENT));
            }
            if resp.clicked() {
                p.event = Some(id.clone());
            }
            if let Some(from) = resp.dnd_release_payload::<usize>() {
                moved = Some((*from, i));
            }
            resp.context_menu(|ui| {
                for (name, key) in [("Дублировать (Ctrl+D)", "dup"), ("Выше", "up"), ("Ниже", "down"), ("Удалить (Del)", "del")] {
                    if ui.button(name).clicked() {
                        action = Some((i, key));
                        ui.close();
                    }
                }
            });
        }
    });
    if let Some((from, to)) = moved {
        if let Some(scene) = p.story.scene_mut(&sid) {
            if from != to && from < scene.events.len() {
                let e = scene.events.remove(from);
                scene.events.insert(to.min(scene.events.len()), e);
            }
        }
    }
    if let Some((i, key)) = action {
        p.event = Some(rows[i].0.clone());
        match key {
            "dup" => duplicate_selected(p),
            "del" => delete_selected(p),
            _ => {
                if let Some(scene) = p.story.scene_mut(&sid) {
                    let j = if key == "up" { i.saturating_sub(1) } else { (i + 1).min(scene.events.len() - 1) };
                    scene.events.swap(i, j);
                }
            }
        }
    }
}
