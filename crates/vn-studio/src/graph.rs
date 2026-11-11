//! Сюжет: сцены-узлы и переходы между ними. Узлы можно двигать, соединять мышью,
//! создавать, копировать и удалять; двойной щелчок открывает сцену.
use crate::app::{Confirm, Project, Tab, ACCENT};
use crate::ops::{self, NODE};
use egui::{pos2, vec2, Color32, Pos2, Rect, Sense, Stroke, Ui};
use std::collections::BTreeSet;
use vn_core::analysis::reachable;
use vn_core::model::*;
use vn_core::templates::duplicate_scene;

fn node_rect(story: &Story, id: &SceneId) -> Option<Rect> {
    story.layout.get(id).map(|p| Rect::from_min_size(pos2(p[0], p[1]), vec2(NODE[0], NODE[1])))
}

fn edge_points(a: Rect, b: Rect) -> (Pos2, Pos2) {
    if a == b {
        (a.right_top() + vec2(-30.0, 0.0), a.right_top() + vec2(-8.0, 0.0))
    } else if b.center().x >= a.center().x {
        (a.right_center(), b.left_center())
    } else {
        (a.center_bottom(), b.center_bottom())
    }
}

fn draw_edge(painter: &egui::Painter, a: Rect, b: Rect, label: &str, color: Color32) {
    let (from, to) = edge_points(a, b);
    let bend = if a == b {
        vec2(0.0, -46.0)
    } else if b.center().x >= a.center().x {
        vec2(((to.x - from.x) * 0.5).max(30.0), 0.0)
    } else {
        vec2(0.0, 60.0)
    };
    let (c1, c2) = if b.center().x >= a.center().x && a != b { (from + bend, to - bend) } else { (from + bend, to + bend) };
    painter.add(egui::epaint::CubicBezierShape::from_points_stroke([from, c1, c2, to], false, Color32::TRANSPARENT, Stroke::new(2.0, color)));
    let dir = (to - c2).normalized();
    let side = vec2(-dir.y, dir.x);
    painter.add(egui::Shape::convex_polygon(vec![to, to - dir * 10.0 + side * 5.0, to - dir * 10.0 - side * 5.0], color, Stroke::NONE));
    if !label.is_empty() {
        let mid = pos2((from.x + to.x) * 0.5 + bend.y * 0.0, (from.y + to.y) * 0.5 + if b.center().x >= a.center().x { 0.0 } else { 45.0 });
        let text: String = if label.chars().count() > 22 { format!("{}…", label.chars().take(22).collect::<String>()) } else { label.to_string() };
        let galley = painter.layout_no_wrap(text, egui::FontId::proportional(11.0), Color32::WHITE);
        let r = Rect::from_center_size(mid, galley.size() + vec2(8.0, 4.0));
        painter.rect_filled(r, 4.0, Color32::from_rgb(40, 38, 70));
        painter.galley(r.min + vec2(4.0, 2.0), galley, Color32::WHITE);
    }
}

fn new_scene_at(p: &mut Project, pos: Pos2) {
    let scene = Scene::new(&format!("Сцена {}", p.story.scenes.len() + 1));
    p.story.layout.insert(scene.id.clone(), [pos.x, pos.y]);
    p.scene = Some(scene.id.clone());
    p.event = None;
    p.story.scenes.push(scene);
}

pub fn ui(ui: &mut Ui, p: &mut Project) {
    ops::auto_layout(&mut p.story, false);
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ Сцена").clicked() {
            let at = p.graph_view.center() - vec2(NODE[0], NODE[1]) * 0.5;
            new_scene_at(p, at);
        }
        if ui.button("Разложить автоматически").clicked() {
            ops::auto_layout(&mut p.story, true);
            p.graph_view = Rect::ZERO;
        }
        if ui.button("Показать всё").clicked() {
            p.graph_view = Rect::ZERO;
        }
        ui.weak("Тяните узел — переместить · тяните за кружок справа — соединить · двойной щелчок — открыть · правая кнопка — меню · колесо — масштаб");
    });
    let start = p.story.meta.start_scene.clone();
    let live: BTreeSet<SceneId> = start.as_ref().map(|s| reachable(&p.story, s)).unwrap_or_default();
    let edges = p.story.edges();
    let nodes: Vec<(SceneId, String, bool, usize)> = p.story.scenes.iter().map(|s| (s.id.clone(), s.name.clone(), s.events.iter().any(|e| matches!(e.kind, EventKind::End { .. })), s.events.len())).collect();
    if p.graph_view == Rect::ZERO {
        let all = nodes.iter().filter_map(|(id, ..)| node_rect(&p.story, id)).reduce(|a, b| a.union(b)).unwrap_or(Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0)));
        p.graph_view = all.expand(80.0);
    }
    let mut view = p.graph_view;
    let center = view.center();
    let mut action: Option<(&str, SceneId)> = None;
    let mut link: Option<(SceneId, SceneId)> = None;
    let mut create_at = None;
    egui::Frame::NONE.fill(Color32::from_gray(20)).show(ui, |ui| {
        egui::Scene::new().zoom_range(0.2..=2.0).show(ui, &mut view, |ui| {
            let painter = ui.painter().clone();
            let bg = ui.interact(Rect::EVERYTHING, ui.scope_id().with("graph-bg"), Sense::click());
            bg.context_menu(|ui| {
                if ui.button("Новая сцена здесь").clicked() {
                    create_at = ui.input(|i| i.pointer.interact_pos()).or(Some(center));
                    ui.close();
                }
            });
            for e in &edges {
                if let (Some(a), Some(b)) = (node_rect(&p.story, &e.from), node_rect(&p.story, &e.to)) {
                    draw_edge(&painter, a, b, &e.label, Color32::from_rgb(150, 140, 230));
                }
            }
            let pointer = ui.input(|i| i.pointer.latest_pos()).map(|pos| ui.ctx().layer_transform_from_global(ui.layer_id()).map(|t| t * pos).unwrap_or(pos));
            for (id, name, ending, count) in &nodes {
                let Some(rect) = node_rect(&p.story, id) else { continue };
                let resp = ui.interact(rect, egui::Id::unique(("node", id)), Sense::click_and_drag());
                let selected = p.scene.as_ref() == Some(id);
                let fill = if !live.contains(id) {
                    Color32::from_rgb(70, 50, 50)
                } else if *ending {
                    Color32::from_rgb(70, 60, 30)
                } else {
                    Color32::from_rgb(38, 40, 70)
                };
                painter.rect(rect, 10.0, fill, Stroke::new(if selected { 3.0 } else { 1.0 }, if selected { ACCENT } else { Color32::from_gray(110) }), egui::StrokeKind::Inside);
                let title: String = if name.chars().count() > 20 { format!("{}…", name.chars().take(20).collect::<String>()) } else { name.clone() };
                let mark = if start.as_ref() == Some(id) { "▶ " } else { "" };
                painter.text(rect.left_top() + vec2(12.0, 12.0), egui::Align2::LEFT_TOP, format!("{mark}{title}"), egui::FontId::proportional(15.0), Color32::WHITE);
                let note = if !live.contains(id) { "недостижима от начала".to_string() } else if *ending { format!("концовка · событий: {count}") } else { format!("событий: {count}") };
                painter.text(rect.left_bottom() + vec2(12.0, -10.0), egui::Align2::LEFT_BOTTOM, note, egui::FontId::proportional(11.0), Color32::from_gray(170));
                if resp.dragged() && !resp.drag_started() {
                    let d = resp.drag_delta();
                    if let Some(pos) = p.story.layout.get_mut(id) {
                        pos[0] += d.x;
                        pos[1] += d.y;
                    }
                }
                if resp.clicked() {
                    p.scene = Some(id.clone());
                    p.event = None;
                }
                if resp.double_clicked() {
                    action = Some(("open", id.clone()));
                }
                resp.context_menu(|ui| {
                    for (label, key) in [("Открыть сцену", "open"), ("Дублировать", "dup"), ("Сделать начальной", "start"), ("Удалить…", "del")] {
                        if ui.button(label).clicked() {
                            action = Some((key, id.clone()));
                            ui.close();
                        }
                    }
                });
                let port = Rect::from_center_size(rect.right_center(), vec2(18.0, 18.0));
                let port_resp = ui.interact(port, egui::Id::unique(("port", id)), Sense::drag()).on_hover_text("Потяните к другой сцене, чтобы добавить переход");
                painter.circle(port.center(), 7.0, if port_resp.hovered() || p.linking.as_ref() == Some(id) { ACCENT } else { Color32::from_gray(90) }, Stroke::new(1.5, Color32::WHITE));
                if port_resp.drag_started() {
                    p.linking = Some(id.clone());
                }
                if p.linking.as_ref() == Some(id) {
                    if let Some(pos) = pointer {
                        painter.line_segment([port.center(), pos], Stroke::new(2.0, ACCENT));
                    }
                    if port_resp.drag_stopped() {
                        if let Some(target) = pointer.and_then(|pos| nodes.iter().find(|(t, ..)| node_rect(&p.story, t).is_some_and(|r| r.contains(pos)))) {
                            link = Some((id.clone(), target.0.clone()));
                        }
                        p.linking = None;
                    }
                }
            }
        });
    });
    p.graph_view = view;
    if let Some(pos) = create_at {
        new_scene_at(p, pos);
    }
    if let Some((from, to)) = link {
        match ops::link_scenes(&mut p.story, &from, &to) {
            Ok(()) => p.status = "Переход добавлен. Текст варианта можно изменить в свойствах события «Выбор».".into(),
            Err(e) => p.status = e,
        }
    }
    match action {
        Some(("open", id)) => {
            p.select(Some(id), None);
            p.tab = Tab::Stage;
        }
        Some(("dup", id)) => {
            if let Some(new) = duplicate_scene(&mut p.story, &id) {
                if let Some(pos) = p.story.layout.get(&id).copied() {
                    p.story.layout.insert(new.clone(), [pos[0] + 30.0, pos[1] + NODE[1] + 20.0]);
                }
                p.select(Some(new), None);
            }
        }
        Some(("start", id)) => p.story.meta.start_scene = Some(id),
        Some((_, id)) => p.confirm = Some(Confirm::Scene { id, redirect: None }),
        None => {}
    }
}
