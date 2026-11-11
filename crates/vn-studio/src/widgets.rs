//! Общие элементы форм редактора: списки выбора сцен, персонажей, файлов, поля условий.
use egui::Ui;
use std::collections::{BTreeMap, BTreeSet};
use vn_core::cond::Cond;
use vn_core::model::*;
use vn_store::project::AssetKind;

/// Списки для полей выбора. Собираются раз за кадр, чтобы формы могли менять историю.
pub struct Lists {
    pub scenes: Vec<(SceneId, String)>,
    pub chars: Vec<(CharId, String)>,
    pub exprs: BTreeMap<CharId, Vec<(String, String)>>,
    pub vars: Vec<String>,
    pub slots: Vec<(SlotId, String)>,
    pub items: Vec<(ItemId, String, SlotId)>,
    pub chapters: Vec<(ChapterId, String)>,
    pub files: BTreeSet<String>,
}

impl Lists {
    pub fn of(story: &Story, files: &BTreeSet<String>) -> Self {
        Self {
            scenes: story.scenes.iter().map(|s| (s.id.clone(), s.name.clone())).collect(),
            chars: story.characters.iter().map(|c| (c.id.clone(), c.name.clone())).collect(),
            exprs: story.characters.iter().map(|c| (c.id.clone(), c.expressions.iter().map(|e| (e.name.clone(), e.name.clone())).collect())).collect(),
            vars: story.variables.iter().map(|v| v.name.clone()).collect(),
            slots: story.wardrobe.slots.iter().map(|s| (s.id.clone(), s.name.clone())).collect(),
            items: story.wardrobe.items.iter().map(|i| (i.id.clone(), i.name.clone(), i.slot.clone())).collect(),
            chapters: story.chapters.iter().map(|c| (c.id.clone(), c.name.clone())).collect(),
            files: files.clone(),
        }
    }

    pub fn assets(&self, kind: AssetKind) -> Vec<(String, String)> {
        let prefix = format!("{}/", kind.folder());
        self.files.iter().filter(|f| f.starts_with(&prefix)).map(|f| (f.clone(), f[prefix.len()..].to_string())).collect()
    }
}

/// Выпадающий список. Возвращает `true`, если значение изменилось.
pub fn pick<T: PartialEq + Clone>(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, current: &mut T, options: &[(T, String)], missing: &str) -> bool {
    let label = options.iter().find(|(v, _)| v == current).map(|(_, n)| n.clone()).unwrap_or_else(|| missing.to_string());
    let mut changed = false;
    egui::ComboBox::from_id_salt(egui::Id::unique(id)).selected_text(label).width(170.0).show_ui(ui, |ui| {
        for (v, n) in options {
            if ui.selectable_label(v == current, n.as_str()).clicked() && v != current {
                *current = v.clone();
                changed = true;
            }
        }
    });
    changed
}

/// Список с пунктом «нет».
pub fn pick_opt<T: PartialEq + Clone>(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, current: &mut Option<T>, options: &[(T, String)], none: &str) -> bool {
    let mut all: Vec<(Option<T>, String)> = vec![(None, none.to_string())];
    all.extend(options.iter().map(|(v, n)| (Some(v.clone()), n.clone())));
    pick(ui, id, current, &all, "— не найдено —")
}

pub fn asset_pick(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, lists: &Lists, kind: AssetKind, current: &mut String) -> bool {
    let options = lists.assets(kind);
    let missing = if current.is_empty() { "— выберите файл —".to_string() } else { format!("нет файла: {}", crate::ops::asset_name(current)) };
    pick(ui, id, current, &options, &missing)
}

pub fn row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) {
    ui.horizontal(|ui| {
        ui.add_sized([110.0, 18.0], egui::Label::new(egui::RichText::new(label).weak()).halign(egui::Align::Min));
        add(ui);
    });
}

pub fn seconds(ui: &mut Ui, value: &mut f32) {
    ui.add(egui::DragValue::new(value).range(0.0..=60.0).speed(0.05).suffix(" с"));
}

pub fn percent(ui: &mut Ui, value: &mut f32) {
    let mut p = *value * 100.0;
    if ui.add(egui::Slider::new(&mut p, 0.0..=100.0).suffix("%").fixed_decimals(0)).changed() {
        *value = p / 100.0;
    }
}

pub fn value_edit(ui: &mut Ui, value: &mut Value) {
    let mut text = value.to_string();
    if ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0).hint_text("5, да, текст")).changed() {
        *value = Value::parse(&text);
    }
}

/// Имя переменной: можно ввести или выбрать из объявленных.
pub fn var_edit(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, lists: &Lists, var: &mut String) {
    ui.add(egui::TextEdit::singleline(var).desired_width(110.0).hint_text("переменная").id_salt(egui::Id::unique(&id)));
    if !lists.vars.is_empty() {
        ui.menu_button("▾", |ui| {
            for v in &lists.vars {
                if ui.button(v).clicked() {
                    *var = v.clone();
                    ui.close();
                }
            }
        });
    }
}

/// Поле условия. Текст хранится во временной памяти интерфейса, чтобы не мешать вводу;
/// в историю попадает только разобранное условие.
pub fn cond_edit(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, cond: &mut Option<Cond>) {
    let id = egui::Id::unique(id).with("cond");
    let shown = cond.as_ref().map(|c| c.to_string()).unwrap_or_default();
    let (mut text, base): (String, String) = ui.data_mut(|d| d.get_temp(id)).unwrap_or_else(|| (shown.clone(), shown.clone()));
    if base != shown {
        text = shown.clone();
    }
    let resp = ui.add(egui::TextEdit::singleline(&mut text).desired_width(230.0).hint_text("например: доверие >= 5 и не ключ"));
    let parsed = if text.trim().is_empty() { Ok(None) } else { Cond::parse(&text).map(Some) };
    let mut base = shown;
    match parsed {
        Ok(new) => {
            if resp.changed() && new != *cond {
                *cond = new;
                base = cond.as_ref().map(|c| c.to_string()).unwrap_or_default();
            }
        }
        Err(e) => {
            ui.colored_label(egui::Color32::LIGHT_RED, format!("⚠ {e}"));
        }
    }
    ui.data_mut(|d| d.insert_temp(id, (text, base)));
}

pub fn pos_edit(ui: &mut Ui, pos: &mut Pos) {
    ui.horizontal(|ui| {
        for (name, preset) in [("Слева", Pos::LEFT), ("Центр", Pos::CENTER), ("Справа", Pos::RIGHT)] {
            if ui.selectable_label((pos.x - preset.x).abs() < 0.005, name).clicked() {
                pos.x = preset.x;
            }
        }
        ui.add(egui::DragValue::new(&mut pos.x).range(0.0..=1.0).speed(0.005).prefix("x "));
        ui.add(egui::DragValue::new(&mut pos.y).range(0.2..=1.6).speed(0.005).prefix("y "));
        ui.add(egui::DragValue::new(&mut pos.scale).range(0.2..=3.0).speed(0.01).prefix("× "));
    });
}

/// Открыть папку или файл в системе.
pub fn reveal(path: &std::path::Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(program).arg(path).spawn();
}
