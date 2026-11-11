//! Форма данных истории. Всё, что нужно движку, лежит здесь и сериализуется в JSON.
//! Позиция игрока и ссылки между сценами держатся только на постоянных ID.
use crate::cond::Cond;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Версия формата файлов проекта. Меняется только при несовместимом изменении структуры.
pub const FORMAT_VERSION: u32 = 1;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
        #[serde(transparent)]
        pub struct $name(pub String);
        impl $name {
            pub fn new(prefix: &str) -> Self {
                Self(new_id(prefix))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }
    };
}
id_type!(SceneId);
id_type!(EventId);
id_type!(CharId);
id_type!(OptionId);
id_type!(ItemId);
id_type!(SlotId);
id_type!(ChapterId);

/// Новый постоянный идентификатор вида `prefix_1a2b3c4d`.
pub fn new_id(prefix: &str) -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    format!("{prefix}_{:08x}", h.finish() as u32)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Text(String),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Bool(true) => f.write_str("да"),
            Value::Bool(false) => f.write_str("нет"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Text(s) => f.write_str(s),
        }
    }
}

impl Value {
    /// Разбор значения из текстового поля редактора.
    pub fn parse(s: &str) -> Value {
        let t = s.trim();
        match t.to_lowercase().as_str() {
            "true" | "да" => Value::Bool(true),
            "false" | "нет" => Value::Bool(false),
            _ => t.parse::<i64>().map(Value::Int).unwrap_or_else(|_| Value::Text(t.to_string())),
        }
    }
}

pub type Vars = BTreeMap<String, Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum VarAction {
    #[default]
    Set,
    Add,
    Sub,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VarOp {
    pub var: String,
    #[serde(default)]
    pub action: VarAction,
    pub value: Value,
}

impl VarOp {
    pub fn apply(&self, vars: &mut Vars) {
        let new = match (self.action, vars.get(&self.var), &self.value) {
            (VarAction::Add, Some(Value::Int(a)), Value::Int(b)) => Value::Int(a.saturating_add(*b)),
            (VarAction::Sub, Some(Value::Int(a)), Value::Int(b)) => Value::Int(a.saturating_sub(*b)),
            (VarAction::Add, None, Value::Int(b)) => Value::Int(*b),
            (VarAction::Sub, None, Value::Int(b)) => Value::Int(-*b),
            _ => self.value.clone(),
        };
        vars.insert(self.var.clone(), new);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VarDef {
    pub name: String,
    pub initial: Value,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextSettings {
    /// Высота шрифта в долях высоты экрана.
    pub size: f32,
    pub color: [u8; 3],
    /// Символов в секунду.
    pub speed: f32,
}

impl Default for TextSettings {
    fn default() -> Self {
        Self { size: 0.038, color: [255, 255, 255], speed: 40.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub format_version: u32,
    /// Постоянный идентификатор истории: по нему игра находит свои сохранения.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    /// Версия для людей, например «1.1».
    pub version: String,
    /// Номер выпуска сценария; растёт с каждым выпущенным обновлением.
    pub story_version: u32,
    pub resolution: [u32; 2],
    #[serde(default)]
    pub fullscreen: bool,
    pub start_scene: Option<SceneId>,
    /// Куда отправить игрока, если сцена его сохранения удалена и для неё нет перенаправления.
    /// `None` — сообщить, что сохранение несовместимо.
    #[serde(default)]
    pub fallback_scene: Option<SceneId>,
    #[serde(default)]
    pub text: TextSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub id: ChapterId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Expression {
    pub name: String,
    pub image: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Character {
    pub id: CharId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub color: [u8; 3],
    /// Выражения лица / варианты спрайта. Первое — по умолчанию.
    #[serde(default)]
    pub expressions: Vec<Expression>,
    /// Высота спрайта в долях высоты экрана при масштабе 1.
    #[serde(default = "default_char_height")]
    pub height: f32,
}

fn default_char_height() -> f32 {
    0.9
}

impl Character {
    pub fn new(name: &str) -> Self {
        Self { id: CharId::new("ch"), name: name.to_string(), description: String::new(), color: [255, 200, 120], expressions: vec![], height: 0.9 }
    }
    pub fn image(&self, expression: &str) -> Option<&str> {
        self.expressions.iter().find(|e| e.name == expression).or(self.expressions.first()).map(|e| e.image.as_str())
    }
}

/// Слот гардероба: причёска, верх, обувь… Порядок слотов в списке — порядок слоёв снизу вверх.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slot {
    pub id: SlotId,
    pub name: String,
    /// Слой рисуется под телом героя (например, задняя часть волос).
    #[serde(default)]
    pub behind: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub slot: SlotId,
    pub name: String,
    pub image: String,
    /// Доступен с начала игры; иначе открывается событием «Открыть предмет».
    #[serde(default)]
    pub unlocked: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Wardrobe {
    pub hero: Option<CharId>,
    #[serde(default)]
    pub slots: Vec<Slot>,
    #[serde(default)]
    pub items: Vec<Item>,
    /// Игрок может открыть гардероб из меню в любой момент.
    #[serde(default)]
    pub anytime: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pos {
    /// Центр спрайта по горизонтали, 0..1.
    pub x: f32,
    /// Низ спрайта по вертикали, 0..1 (1 — нижний край экрана).
    pub y: f32,
    pub scale: f32,
}

impl Pos {
    pub const LEFT: Pos = Pos { x: 0.22, y: 1.0, scale: 1.0 };
    pub const CENTER: Pos = Pos { x: 0.5, y: 1.0, scale: 1.0 };
    pub const RIGHT: Pos = Pos { x: 0.78, y: 1.0, scale: 1.0 };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TextStyle {
    #[default]
    Dialogue,
    Thought,
    Narration,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DialogPos {
    Top,
    Center,
    #[default]
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub id: OptionId,
    pub text: String,
    /// Вариант виден только при выполнении условия.
    #[serde(default)]
    pub cond: Option<Cond>,
    #[serde(default)]
    pub effects: Vec<VarOp>,
    /// `None` — продолжить текущую сцену.
    #[serde(default)]
    pub goto: Option<SceneId>,
}

impl ChoiceOption {
    pub fn new(text: &str, goto: Option<SceneId>) -> Self {
        Self { id: OptionId::new("op"), text: text.to_string(), cond: None, effects: vec![], goto }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    Background { image: String, fade: f32 },
    ShowCharacter { character: CharId, expression: String, pos: Pos, fade: f32 },
    HideCharacter { character: CharId, fade: f32 },
    MoveCharacter { character: CharId, pos: Pos, duration: f32 },
    Expression { character: CharId, expression: String },
    SetOutfit { slot: SlotId, item: Option<ItemId> },
    UnlockItem { item: ItemId },
    /// Открыть гардероб игроку; пустой список слотов — доступны все.
    OpenWardrobe { slots: Vec<SlotId> },
    Say {
        speaker: Option<CharId>,
        #[serde(default)]
        style: TextStyle,
        text: String,
        #[serde(default)]
        expression: Option<String>,
        /// Своя скорость появления текста, символов в секунду.
        #[serde(default)]
        speed: Option<f32>,
        #[serde(default)]
        sound: Option<String>,
    },
    DialogBox { pos: DialogPos },
    PlayMusic { track: String, volume: f32, fade: f32, looped: bool },
    StopMusic { fade: f32 },
    MusicVolume { volume: f32, fade: f32 },
    Sound { file: String, volume: f32 },
    Wait { seconds: f32 },
    /// Затемнить экран (`out = true`) или вернуть изображение.
    Fade { out: bool, duration: f32 },
    Goto { scene: SceneId },
    Choice { prompt: String, options: Vec<ChoiceOption> },
    SetVar(VarOp),
    /// Если условие истинно — перейти к сцене, иначе продолжить.
    Branch { cond: Cond, scene: SceneId },
    End { title: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    /// Пауза перед событием, секунд.
    #[serde(default)]
    pub delay: f32,
    #[serde(flatten)]
    pub kind: EventKind,
}

impl Event {
    pub fn new(kind: EventKind) -> Self {
        Self { id: EventId::new("ev"), delay: 0.0, kind }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub id: SceneId,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub chapter: Option<ChapterId>,
    #[serde(default)]
    pub events: Vec<Event>,
}

impl Scene {
    pub fn new(name: &str) -> Self {
        Self { id: SceneId::new("sc"), name: name.to_string(), description: String::new(), chapter: None, events: vec![] }
    }
    pub fn event_index(&self, id: &EventId) -> Option<usize> {
        self.events.iter().position(|e| &e.id == id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Story {
    pub meta: Meta,
    #[serde(default)]
    pub chapters: Vec<Chapter>,
    #[serde(default)]
    pub scenes: Vec<Scene>,
    #[serde(default)]
    pub characters: Vec<Character>,
    #[serde(default)]
    pub variables: Vec<VarDef>,
    #[serde(default)]
    pub wardrobe: Wardrobe,
    /// Удалённая сцена → сцена, с которой продолжат игроки, сохранившиеся в удалённой.
    #[serde(default)]
    pub redirects: BTreeMap<SceneId, SceneId>,
    /// Положение узлов в редакторе сюжета. Движку не нужно.
    #[serde(default)]
    pub layout: BTreeMap<SceneId, [f32; 2]>,
}

/// Переход из сцены: откуда, каким событием, с какой подписью и куда.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: SceneId,
    pub event: EventId,
    pub label: String,
    pub to: SceneId,
}

impl Story {
    pub fn new(name: &str) -> Self {
        let first = Scene::new("Начало");
        Story {
            meta: Meta {
                format_version: FORMAT_VERSION,
                id: new_id("story"),
                name: name.to_string(),
                description: String::new(),
                author: String::new(),
                version: "1.0".into(),
                story_version: 1,
                resolution: [1280, 720],
                fullscreen: false,
                start_scene: Some(first.id.clone()),
                fallback_scene: None,
                text: TextSettings::default(),
            },
            chapters: vec![],
            scenes: vec![first],
            characters: vec![],
            variables: vec![],
            wardrobe: Wardrobe::default(),
            redirects: BTreeMap::new(),
            layout: BTreeMap::new(),
        }
    }
    pub fn scene(&self, id: &SceneId) -> Option<&Scene> {
        self.scenes.iter().find(|s| &s.id == id)
    }
    pub fn scene_mut(&mut self, id: &SceneId) -> Option<&mut Scene> {
        self.scenes.iter_mut().find(|s| &s.id == id)
    }
    pub fn character(&self, id: &CharId) -> Option<&Character> {
        self.characters.iter().find(|c| &c.id == id)
    }
    pub fn item(&self, id: &ItemId) -> Option<&Item> {
        self.wardrobe.items.iter().find(|i| &i.id == id)
    }
    pub fn initial_vars(&self) -> Vars {
        self.variables.iter().map(|v| (v.name.clone(), v.initial.clone())).collect()
    }

    /// Все переходы между сценами.
    pub fn edges(&self) -> Vec<Edge> {
        let mut out = vec![];
        for s in &self.scenes {
            for e in &s.events {
                let mut push = |label: String, to: &SceneId| out.push(Edge { from: s.id.clone(), event: e.id.clone(), label, to: to.clone() });
                match &e.kind {
                    EventKind::Goto { scene } => push(String::new(), scene),
                    EventKind::Branch { cond, scene } => push(format!("если {cond}"), scene),
                    EventKind::Choice { options, .. } => {
                        for o in options {
                            if let Some(to) = &o.goto {
                                push(o.text.clone(), to)
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// Все файлы ресурсов, на которые ссылается история.
    pub fn assets_used(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for c in &self.characters {
            out.extend(c.expressions.iter().map(|e| e.image.clone()));
        }
        out.extend(self.wardrobe.items.iter().map(|i| i.image.clone()));
        for e in self.scenes.iter().flat_map(|s| &s.events) {
            match &e.kind {
                EventKind::Background { image, .. } => drop(out.insert(image.clone())),
                EventKind::PlayMusic { track, .. } => drop(out.insert(track.clone())),
                EventKind::Sound { file, .. } => drop(out.insert(file.clone())),
                EventKind::Say { sound: Some(s), .. } => drop(out.insert(s.clone())),
                _ => {}
            }
        }
        out.remove("");
        out
    }

    /// Удалить сцену; игроки, сохранившиеся в ней, продолжат со сцены `redirect`.
    pub fn remove_scene(&mut self, id: &SceneId, redirect: Option<SceneId>) {
        self.scenes.retain(|s| &s.id != id);
        self.layout.remove(id);
        for target in self.redirects.values_mut() {
            if target == id {
                if let Some(r) = &redirect {
                    *target = r.clone();
                }
            }
        }
        self.redirects.retain(|_, to| to != id);
        if let Some(r) = redirect {
            self.redirects.insert(id.clone(), r);
        }
        if self.meta.start_scene.as_ref() == Some(id) {
            self.meta.start_scene = self.scenes.first().map(|s| s.id.clone());
        }
        if self.meta.fallback_scene.as_ref() == Some(id) {
            self.meta.fallback_scene = None;
        }
    }
}

impl EventKind {
    /// Название типа события для списков редактора.
    pub fn title(&self) -> &'static str {
        match self {
            EventKind::Background { .. } => "Фон",
            EventKind::ShowCharacter { .. } => "Показать персонажа",
            EventKind::HideCharacter { .. } => "Скрыть персонажа",
            EventKind::MoveCharacter { .. } => "Переместить персонажа",
            EventKind::Expression { .. } => "Выражение лица",
            EventKind::SetOutfit { .. } => "Сменить одежду",
            EventKind::UnlockItem { .. } => "Открыть предмет",
            EventKind::OpenWardrobe { .. } => "Гардероб",
            EventKind::Say { .. } => "Текст",
            EventKind::DialogBox { .. } => "Окно диалога",
            EventKind::PlayMusic { .. } => "Музыка",
            EventKind::StopMusic { .. } => "Остановить музыку",
            EventKind::MusicVolume { .. } => "Громкость музыки",
            EventKind::Sound { .. } => "Звук",
            EventKind::Wait { .. } => "Пауза",
            EventKind::Fade { out: true, .. } => "Затемнение",
            EventKind::Fade { out: false, .. } => "Проявление",
            EventKind::Goto { .. } => "Переход к сцене",
            EventKind::Choice { .. } => "Выбор",
            EventKind::SetVar(_) => "Переменная",
            EventKind::Branch { .. } => "Условие",
            EventKind::End { .. } => "Концовка",
        }
    }

    /// Событие останавливает сцену: после него следующие события не выполняются.
    pub fn is_terminal(&self) -> bool {
        matches!(self, EventKind::Goto { .. } | EventKind::End { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_json_is_flat_and_round_trips() {
        let e = Event { id: "ev_1".into(), delay: 0.5, kind: EventKind::Goto { scene: "sc_2".into() } };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json, serde_json::json!({"id":"ev_1","delay":0.5,"type":"goto","scene":"sc_2"}));
        assert_eq!(serde_json::from_value::<Event>(json).unwrap(), e);
    }

    #[test]
    fn value_parses_from_editor_text() {
        assert_eq!(Value::parse(" 12 "), Value::Int(12));
        assert_eq!(Value::parse("Да"), Value::Bool(true));
        assert_eq!(Value::parse("outfit_03"), Value::Text("outfit_03".into()));
    }

    #[test]
    fn var_ops_change_numbers() {
        let mut vars = Vars::new();
        VarOp { var: "money".into(), action: VarAction::Set, value: Value::Int(100) }.apply(&mut vars);
        VarOp { var: "money".into(), action: VarAction::Sub, value: Value::Int(30) }.apply(&mut vars);
        VarOp { var: "trust".into(), action: VarAction::Add, value: Value::Int(2) }.apply(&mut vars);
        assert_eq!(vars["money"], Value::Int(70));
        assert_eq!(vars["trust"], Value::Int(2));
    }

    #[test]
    fn removing_scene_records_redirect_and_fixes_start() {
        let mut s = Story::new("t");
        let first = s.scenes[0].id.clone();
        let second = Scene::new("B");
        let second_id = second.id.clone();
        s.scenes.push(second);
        s.remove_scene(&first, Some(second_id.clone()));
        assert_eq!(s.redirects.get(&first), Some(&second_id));
        assert_eq!(s.meta.start_scene, Some(second_id));
    }

    #[test]
    fn edges_list_choice_and_goto_targets() {
        let mut s = Story::new("t");
        let a = s.scenes[0].id.clone();
        s.scenes[0].events.push(Event::new(EventKind::Choice { prompt: String::new(), options: vec![ChoiceOption::new("Помочь", Some("x".into())), ChoiceOption::new("Молчать", None)] }));
        s.scenes[0].events.push(Event::new(EventKind::Goto { scene: "y".into() }));
        let e = s.edges();
        assert_eq!(e.iter().map(|e| (e.from.clone(), e.label.as_str(), e.to.as_str())).collect::<Vec<_>>(), vec![(a.clone(), "Помочь", "x"), (a, "", "y")]);
    }
}
