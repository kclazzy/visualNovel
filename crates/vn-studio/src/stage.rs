//! Сцена в центре редактора: как выглядит экран после выбранного события.
//! Персонажей можно двигать мышью, фон и персонажей — перетаскивать из левой панели.
use crate::app::{Dragged, Project};
use crate::{ops, timeline};
use egui::{Color32, Sense, Ui};
use vn_core::model::*;
use vn_core::runtime::{stage_at, Stage};
use vn_core::view::{spans, sprites, Speaker};
use vn_store::project::AssetKind;
use vn_view::draw;

fn dropped(p: &mut Project, what: &Dragged, x: f32) {
    match what {
        Dragged::Character(id) => {
            let expression = p.story.character(id).and_then(|c| c.expressions.first()).map(|e| e.name.clone()).unwrap_or_default();
            timeline::add_events(p, vec![Event::new(EventKind::ShowCharacter { character: id.clone(), expression, pos: Pos { x: x.clamp(0.05, 0.95), ..Pos::CENTER }, fade: 0.3 })]);
        }
        Dragged::Asset(path) => match AssetKind::of(path) {
            Some(AssetKind::Background) => {
                let selected = p.event_index();
                let scene = p.scene.clone().and_then(|id| p.story.scene_mut(&id));
                match scene.and_then(|s| selected.and_then(|i| s.events.get_mut(i))).map(|e| &mut e.kind) {
                    Some(EventKind::Background { image, .. }) => *image = path.clone(),
                    _ => timeline::add_events(p, vec![Event::new(EventKind::Background { image: path.clone(), fade: 0.5 })]),
                }
            }
            Some(AssetKind::Music) => timeline::add_events(p, vec![Event::new(EventKind::PlayMusic { track: path.clone(), volume: 0.8, fade: 1.0, looped: true })]),
            Some(AssetKind::Sound) => timeline::add_events(p, vec![Event::new(EventKind::Sound { file: path.clone(), volume: 1.0 })]),
            _ => p.status = "Спрайты назначаются персонажу во вкладке «Персонажи», одежда — во вкладке «Гардероб».".into(),
        },
    }
}

pub fn ui(ui: &mut Ui, p: &mut Project) {
    let ctx = ui.ctx().clone();
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
    painter.rect_filled(resp.rect, 0.0, Color32::from_gray(18));
    let stage = draw::fit(resp.rect.shrink(6.0), p.story.meta.resolution);
    painter.rect_filled(stage, 0.0, Color32::BLACK);
    let Some(scene) = p.current_scene().cloned() else { return };
    let selected = p.event_index();
    let upto = selected.unwrap_or(scene.events.len().saturating_sub(1));
    let st = if scene.events.is_empty() { Stage::default() } else { stage_at(&scene, upto) };
    let painter = painter.with_clip_rect(stage);
    match &st.background {
        Some(bg) => draw::background(&painter, &ctx, &mut p.textures, stage, bg, 1.0),
        None => drop(painter.text(stage.center(), egui::Align2::CENTER_CENTER, "Перетащите сюда фон из вкладки «Ресурсы»", egui::FontId::proportional(16.0), Color32::GRAY)),
    }
    let scroll = ui.input(|i| i.smooth_scroll_delta.y);
    let mut outfit = std::collections::BTreeMap::new();
    for e in scene.events.iter().take(upto + 1) {
        match &e.kind {
            EventKind::SetOutfit { slot, item: Some(item) } => drop(outfit.insert(slot.clone(), item.clone())),
            EventKind::SetOutfit { slot, item: None } => drop(outfit.remove(slot)),
            _ => {}
        }
    }
    for sp in sprites(&p.story, &st, &outfit) {
        let rect = draw::sprite(&painter, &ctx, &mut p.textures, stage, &sp.layers, [sp.x, sp.y, sp.height], 1.0).unwrap_or_else(|| {
            let (name, color) = p.story.character(&sp.id).map(|c| (c.name.clone(), c.color)).unwrap_or_default();
            draw::placeholder(&painter, stage, &name, color, sp.x, sp.y, sp.height)
        });
        let r = ui.interact(rect.intersect(stage), egui::Id::unique(("sprite", &sp.id)), Sense::drag()).on_hover_cursor(egui::CursorIcon::Grab);
        // В кадре начала перетаскивания смещение указателя ещё не относится к спрайту.
        let delta = if r.drag_started() { egui::Vec2::ZERO } else { r.drag_delta() };
        let zoom = if r.hovered() && scroll != 0.0 { (scroll * 0.002).exp() } else { 1.0 };
        if r.hovered() || r.dragged() {
            painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.5, crate::app::ACCENT), egui::StrokeKind::Inside);
        }
        if delta != egui::Vec2::ZERO || zoom != 1.0 {
            let moved = p.scene.clone().and_then(|id| p.story.scene_mut(&id)).is_some_and(|s| {
                ops::place_character(s, upto, &sp.id, |pos| {
                    pos.x += delta.x / stage.width();
                    pos.y += delta.y / stage.height();
                    pos.scale *= zoom;
                })
            });
            if !moved {
                p.status = "Положение задаётся событием «Показать персонажа» — его нет в этой сцене до выбранного события.".into();
            }
        }
    }
    if st.black {
        painter.rect_filled(stage, 0.0, Color32::from_black_alpha(190));
        painter.text(stage.center_top() + egui::vec2(0.0, 30.0), egui::Align2::CENTER_CENTER, "экран затемнён", egui::FontId::proportional(14.0), Color32::GRAY);
    }
    match selected.and_then(|i| scene.events.get(i)).map(|e| &e.kind) {
        Some(EventKind::Say { speaker, style, text, .. }) => {
            let who = speaker.as_ref().and_then(|id| p.story.character(id)).map(|c| Speaker { name: c.name.clone(), color: c.color });
            let parts = spans(text, &p.story.initial_vars());
            draw::dialog(&painter, stage, st.dialog, &draw::Line { speaker: who.as_ref(), style: *style, spans: &parts, shown: draw::char_count(&parts) }, &p.story.meta.text);
        }
        Some(EventKind::Choice { prompt, options }) => {
            let unit = stage.height();
            let mut y = stage.center().y - options.len() as f32 * unit * 0.045;
            if !prompt.is_empty() {
                painter.text(egui::pos2(stage.center().x, y - unit * 0.07), egui::Align2::CENTER_CENTER, prompt, egui::FontId::proportional(unit * 0.04), Color32::WHITE);
            }
            for o in options {
                let r = egui::Rect::from_center_size(egui::pos2(stage.center().x, y), egui::vec2(stage.width() * 0.6, unit * 0.075));
                painter.rect(r, unit * 0.02, draw::PANEL, egui::Stroke::new(1.0, draw::ACCENT), egui::StrokeKind::Inside);
                painter.text(r.center(), egui::Align2::CENTER_CENTER, &o.text, egui::FontId::proportional(unit * 0.034), Color32::WHITE);
                y += unit * 0.09;
            }
        }
        Some(EventKind::End { title }) => {
            painter.rect_filled(stage, 0.0, Color32::from_black_alpha(150));
            painter.text(stage.center(), egui::Align2::CENTER_CENTER, title, egui::FontId::proportional(stage.height() * 0.07), draw::GOLD);
        }
        _ => {}
    }
    let caption = match selected {
        Some(i) => format!("{} — после события {}", scene.name, i + 1),
        None => format!("{} — конец сцены", scene.name),
    };
    ui.painter().text(resp.rect.left_top() + egui::vec2(10.0, 6.0), egui::Align2::LEFT_TOP, caption, egui::FontId::proportional(12.0), Color32::from_gray(170));
    if resp.dnd_hover_payload::<Dragged>().is_some() {
        ui.painter().rect_stroke(stage, 0.0, egui::Stroke::new(3.0, crate::app::ACCENT), egui::StrokeKind::Inside);
    }
    if let Some(what) = resp.dnd_release_payload::<Dragged>() {
        let x = ui.input(|i| i.pointer.interact_pos()).map(|pos| draw::stage_point(stage, pos).0).unwrap_or(0.5);
        dropped(p, &what, x);
    }
}
