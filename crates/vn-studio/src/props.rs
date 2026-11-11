//! Свойства выбранного события.
use crate::app::Project;
use crate::widgets::{self, asset_pick, cond_edit, percent, pick, pick_opt, pos_edit, row, seconds, value_edit, var_edit, Lists};
use egui::Ui;
use vn_core::model::*;
use vn_store::project::AssetKind;

fn char_pick(ui: &mut Ui, id: &EventId, lists: &Lists, character: &mut CharId) {
    row(ui, "Персонаж", |ui| pick(ui, (id, "char"), character, &lists.chars, "— выберите персонажа —"));
    if lists.chars.is_empty() {
        ui.weak("Персонажей пока нет: создайте их во вкладке «Персонажи» слева.");
    }
}

fn expr_pick(ui: &mut Ui, id: &EventId, lists: &Lists, character: &CharId, expression: &mut String) {
    let options = lists.exprs.get(character).cloned().unwrap_or_default();
    row(ui, "Выражение", |ui| pick(ui, (id, "expr"), expression, &options, if options.is_empty() { "нет изображений" } else { "— выберите —" }));
}

fn var_op_edit(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug + Copy, lists: &Lists, op: &mut VarOp) {
    var_edit(ui, (id, "var"), lists, &mut op.var);
    pick(ui, (id, "act"), &mut op.action, &[(VarAction::Set, "=".to_string()), (VarAction::Add, "прибавить".to_string()), (VarAction::Sub, "вычесть".to_string())], "?");
    value_edit(ui, &mut op.value);
}

fn choice_edit(ui: &mut Ui, id: &EventId, lists: &Lists, prompt: &mut String, options: &mut Vec<ChoiceOption>) {
    row(ui, "Вопрос", |ui| ui.add(egui::TextEdit::singleline(prompt).desired_width(f32::INFINITY).hint_text("необязательно")));
    let mut remove = None;
    for (n, o) in options.iter_mut().enumerate() {
        let oid = o.id.clone();
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong(format!("{}.", n + 1));
                ui.add(egui::TextEdit::singleline(&mut o.text).desired_width(260.0).hint_text("Текст варианта"));
                ui.label("→");
                pick_opt(ui, (&oid, "goto"), &mut o.goto, &lists.scenes, "продолжить эту сцену");
                if ui.small_button("✖").on_hover_text("Удалить вариант").clicked() {
                    remove = Some(n);
                }
            });
            row(ui, "Виден, если", |ui| cond_edit(ui, (&oid, "cond"), &mut o.cond));
            let mut drop_effect = None;
            for (k, op) in o.effects.iter_mut().enumerate() {
                row(ui, "Изменить", |ui| {
                    var_op_edit(ui, (&oid, k), lists, op);
                    if ui.small_button("✖").clicked() {
                        drop_effect = Some(k);
                    }
                });
            }
            if let Some(k) = drop_effect {
                o.effects.remove(k);
            }
            if ui.small_button("+ изменить переменную при выборе").clicked() {
                o.effects.push(VarOp { var: lists.vars.first().cloned().unwrap_or_default(), action: VarAction::Add, value: Value::Int(1) });
            }
        });
    }
    if let Some(n) = remove {
        options.remove(n);
    }
    if ui.button("+ Вариант").clicked() {
        options.push(ChoiceOption::new("Новый вариант", None));
    }
    let _ = id;
}

fn kind_edit(ui: &mut Ui, id: &EventId, lists: &Lists, kind: &mut EventKind) {
    match kind {
        EventKind::Background { image, fade } => {
            row(ui, "Изображение", |ui| asset_pick(ui, (id, "bg"), lists, AssetKind::Background, image));
            row(ui, "Плавно за", |ui| seconds(ui, fade));
        }
        EventKind::ShowCharacter { character, expression, pos, fade } => {
            char_pick(ui, id, lists, character);
            expr_pick(ui, id, lists, character, expression);
            row(ui, "Положение", |ui| pos_edit(ui, pos));
            row(ui, "Появление за", |ui| seconds(ui, fade));
            ui.weak("Персонажа можно двигать мышью прямо на сцене; колесо мыши меняет размер.");
        }
        EventKind::HideCharacter { character, fade } => {
            char_pick(ui, id, lists, character);
            row(ui, "Исчезает за", |ui| seconds(ui, fade));
        }
        EventKind::MoveCharacter { character, pos, duration } => {
            char_pick(ui, id, lists, character);
            row(ui, "Куда", |ui| pos_edit(ui, pos));
            row(ui, "Длительность", |ui| seconds(ui, duration));
        }
        EventKind::Expression { character, expression } => {
            char_pick(ui, id, lists, character);
            expr_pick(ui, id, lists, character, expression);
        }
        EventKind::SetOutfit { slot, item } => {
            row(ui, "Слот", |ui| pick(ui, (id, "slot"), slot, &lists.slots, "— выберите слот —"));
            let items: Vec<(ItemId, String)> = lists.items.iter().filter(|(_, _, s)| s == slot).map(|(i, n, _)| (i.clone(), n.clone())).collect();
            row(ui, "Предмет", |ui| pick_opt(ui, (id, "item"), item, &items, "снять"));
        }
        EventKind::UnlockItem { item } => {
            let items: Vec<(ItemId, String)> = lists.items.iter().map(|(i, n, _)| (i.clone(), n.clone())).collect();
            row(ui, "Предмет", |ui| pick(ui, (id, "item"), item, &items, "— выберите предмет —"));
            ui.weak("После этого события предмет появится в гардеробе игрока.");
        }
        EventKind::OpenWardrobe { slots } => {
            ui.label("Игрок выбирает одежду. Отметьте слоты, которые можно менять (ничего не отмечено — все):");
            for (sid, name) in &lists.slots {
                let mut on = slots.contains(sid);
                if ui.checkbox(&mut on, name.as_str()).changed() {
                    if on {
                        slots.push(sid.clone());
                    } else {
                        slots.retain(|s| s != sid);
                    }
                }
            }
        }
        EventKind::Say { speaker, style, text, expression, speed, sound } => {
            ui.horizontal(|ui| {
                ui.weak("Говорит");
                if pick_opt(ui, (id, "speaker"), speaker, &lists.chars, "автор (без имени)") {
                    *style = if speaker.is_some() { TextStyle::Dialogue } else { TextStyle::Narration };
                    *expression = None;
                }
                ui.weak("Вид");
                pick(ui, (id, "style"), style, &[(TextStyle::Dialogue, "реплика".to_string()), (TextStyle::Thought, "мысли".to_string()), (TextStyle::Narration, "повествование".to_string()), (TextStyle::System, "системное сообщение".to_string())], "?");
                if let Some(ch) = speaker {
                    let mut options: Vec<(Option<String>, String)> = vec![(None, "не менять лицо".to_string())];
                    options.extend(lists.exprs.get(ch).into_iter().flatten().map(|(e, n)| (Some(e.clone()), n.clone())));
                    pick(ui, (id, "say-expr"), expression, &options, "нет такого выражения");
                }
            });
            ui.add(egui::TextEdit::multiline(text).desired_width(f32::INFINITY).desired_rows(3).hint_text("Текст. {переменная} подставит значение, *слово* выделит важное."));
            ui.horizontal(|ui| {
                let mut own = speed.is_some();
                if ui.checkbox(&mut own, "своя скорость текста").changed() {
                    *speed = own.then_some(40.0);
                }
                if let Some(s) = speed {
                    ui.add(egui::DragValue::new(s).range(5.0..=200.0).suffix(" симв/с"));
                }
                ui.weak("Звук");
                let sounds = lists.assets(AssetKind::Sound);
                pick_opt(ui, (id, "say-sound"), sound, &sounds, "без звука");
            });
        }
        EventKind::DialogBox { pos } => {
            row(ui, "Положение", |ui| pick(ui, (id, "dlg"), pos, &[(DialogPos::Top, "сверху".to_string()), (DialogPos::Center, "по центру".to_string()), (DialogPos::Bottom, "снизу".to_string())], "?"));
        }
        EventKind::PlayMusic { track, volume, fade, looped } => {
            row(ui, "Композиция", |ui| asset_pick(ui, (id, "music"), lists, AssetKind::Music, track));
            row(ui, "Громкость", |ui| percent(ui, volume));
            row(ui, "Нарастание за", |ui| seconds(ui, fade));
            ui.checkbox(looped, "повторять по кругу");
        }
        EventKind::StopMusic { fade } => row(ui, "Затухание за", |ui| seconds(ui, fade)),
        EventKind::MusicVolume { volume, fade } => {
            row(ui, "Громкость", |ui| percent(ui, volume));
            row(ui, "Плавно за", |ui| seconds(ui, fade));
        }
        EventKind::Sound { file, volume } => {
            row(ui, "Звук", |ui| asset_pick(ui, (id, "sound"), lists, AssetKind::Sound, file));
            row(ui, "Громкость", |ui| percent(ui, volume));
        }
        EventKind::Wait { seconds: s } => row(ui, "Длительность", |ui| seconds(ui, s)),
        EventKind::Fade { out, duration } => {
            ui.horizontal(|ui| {
                ui.radio_value(out, true, "затемнить экран");
                ui.radio_value(out, false, "вернуть изображение");
            });
            row(ui, "Длительность", |ui| seconds(ui, duration));
        }
        EventKind::Goto { scene } => {
            row(ui, "К сцене", |ui| pick(ui, (id, "goto"), scene, &lists.scenes, "⚠ сцена не существует"));
        }
        EventKind::Choice { prompt, options } => choice_edit(ui, id, lists, prompt, options),
        EventKind::SetVar(op) => {
            ui.horizontal(|ui| var_op_edit(ui, (id, "setvar"), lists, op));
            if !lists.vars.contains(&op.var) {
                ui.weak("Такой переменной ещё нет: объявите её во вкладке «Переменные» слева.");
            }
        }
        EventKind::Branch { cond, scene } => {
            let mut c = Some(cond.clone());
            row(ui, "Если", |ui| cond_edit(ui, (id, "branch"), &mut c));
            if let Some(c) = c {
                *cond = c;
            }
            row(ui, "перейти к", |ui| pick(ui, (id, "branch-to"), scene, &lists.scenes, "⚠ сцена не существует"));
            ui.weak("Иначе сцена продолжается со следующего события. Можно писать «и», «или», «не», скобки; сравнения: == != > < >= <=.");
        }
        EventKind::End { title } => {
            row(ui, "Название", |ui| ui.add(egui::TextEdit::singleline(title).desired_width(260.0).hint_text("Название концовки")));
        }
    }
}

pub fn ui(ui: &mut Ui, p: &mut Project) {
    let lists = Lists::of(&p.story, &p.files);
    let (Some(sid), Some(i)) = (p.scene.clone(), p.event_index()) else {
        ui.add_space(8.0);
        ui.weak("Выберите событие в таймлайне справа, чтобы изменить его. Новое событие добавляет кнопка «+ Событие».");
        return;
    };
    let Some(event) = p.story.scene_mut(&sid).and_then(|s| s.events.get_mut(i)) else { return };
    let id = event.id.clone();
    ui.horizontal(|ui| {
        ui.strong(event.kind.title());
        ui.separator();
        ui.weak("пауза перед событием");
        widgets::seconds(ui, &mut event.delay);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.weak(format!("ID: {id}")));
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| kind_edit(ui, &id, &lists, &mut event.kind));
}
