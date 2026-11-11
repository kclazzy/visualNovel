//! Исполнение истории: чистый автомат без отрисовки, звука и файлов.
//! Его состояние (`GameState`) — это и есть содержимое сохранения.
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shown {
    pub character: CharId,
    pub expression: String,
    pub pos: Pos,
    /// Длительность появления, секунд.
    pub fade: f32,
    /// Длительность последнего перемещения, секунд.
    pub move_time: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Music {
    pub track: String,
    pub volume: f32,
    pub looped: bool,
    pub fade: f32,
}

/// Что сейчас на экране и в колонках.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Stage {
    pub background: Option<String>,
    pub bg_fade: f32,
    pub characters: Vec<Shown>,
    pub hide_fade: f32,
    pub music: Option<Music>,
    pub music_fade: f32,
    pub dialog: DialogPos,
    pub black: bool,
    pub fade: f32,
}

impl Stage {
    /// Применить видимый/слышимый эффект события. Остальные события сцену не меняют.
    pub fn apply(&mut self, kind: &EventKind) {
        match kind {
            EventKind::Background { image, fade } => {
                self.background = Some(image.clone()).filter(|i| !i.is_empty());
                self.bg_fade = *fade;
            }
            EventKind::ShowCharacter { character, expression, pos, fade } => {
                self.characters.retain(|c| &c.character != character);
                self.characters.push(Shown { character: character.clone(), expression: expression.clone(), pos: *pos, fade: *fade, move_time: 0.0 });
            }
            EventKind::HideCharacter { character, fade } => {
                self.characters.retain(|c| &c.character != character);
                self.hide_fade = *fade;
            }
            EventKind::MoveCharacter { character, pos, duration } => {
                if let Some(c) = self.shown_mut(character) {
                    c.pos = *pos;
                    c.move_time = *duration;
                }
            }
            EventKind::Expression { character, expression } => self.set_expression(character, expression),
            EventKind::Say { speaker: Some(who), expression: Some(expr), .. } => self.set_expression(who, expr),
            EventKind::DialogBox { pos } => self.dialog = *pos,
            EventKind::PlayMusic { track, volume, fade, looped } => {
                self.music = Some(Music { track: track.clone(), volume: *volume, looped: *looped, fade: *fade });
                self.music_fade = *fade;
            }
            EventKind::StopMusic { fade } => {
                self.music = None;
                self.music_fade = *fade;
            }
            EventKind::MusicVolume { volume, fade } => {
                if let Some(m) = &mut self.music {
                    m.volume = *volume;
                }
                self.music_fade = *fade;
            }
            EventKind::Fade { out, duration } => {
                self.black = *out;
                self.fade = *duration;
            }
            _ => {}
        }
    }

    fn shown_mut(&mut self, id: &CharId) -> Option<&mut Shown> {
        self.characters.iter_mut().find(|c| &c.character == id)
    }

    fn set_expression(&mut self, id: &CharId, expression: &str) {
        if let Some(c) = self.shown_mut(id) {
            c.expression = expression.to_string();
        }
    }
}

/// Сцена, какой она выглядит после событий `0..=upto` (для предпросмотра в редакторе).
pub fn stage_at(scene: &Scene, upto: usize) -> Stage {
    let mut stage = Stage::default();
    for e in scene.events.iter().take(upto + 1) {
        stage.apply(&e.kind);
    }
    stage
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChoiceRecord {
    pub scene: SceneId,
    pub event: EventId,
    pub option: OptionId,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogLine {
    pub speaker: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameState {
    pub scene: SceneId,
    /// Событие, на котором стоит игрок. `None` — сцена ещё не начата.
    pub event: Option<EventId>,
    pub vars: Vars,
    #[serde(default)]
    pub choices: Vec<ChoiceRecord>,
    #[serde(default)]
    pub backlog: Vec<LogLine>,
    #[serde(default)]
    pub stage: Stage,
    #[serde(default)]
    pub outfit: BTreeMap<SlotId, ItemId>,
    #[serde(default)]
    pub unlocked: BTreeSet<ItemId>,
    /// Прочитанные реплики — для пропуска уже виденного текста.
    #[serde(default)]
    pub seen: BTreeSet<EventId>,
}

/// Чего движок ждёт от игрока или от времени.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    Text,
    Choice,
    /// Пауза перед событием.
    Delay(f32),
    Wait(f32),
    Wardrobe(Vec<SlotId>),
    Ended(String),
    Error(String),
}

/// Разовый эффект, который нельзя вывести из состояния сцены.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cue {
    pub sound: String,
    pub volume: f32,
}

const STEP_LIMIT: usize = 10_000;
const BACKLOG_LIMIT: usize = 500;

#[derive(Clone, Debug)]
pub struct Runtime {
    pub story: Story,
    pub state: GameState,
    pub(crate) idx: usize,
    pub(crate) pending: Pending,
    cues: Vec<Cue>,
}

impl Runtime {
    /// Новая игра с начальной сцены.
    pub fn new(story: Story) -> Self {
        let start = story.meta.start_scene.clone().unwrap_or_default();
        Self::start_at(story, start)
    }

    /// Новая игра с выбранной сцены (предпросмотр из редактора).
    pub fn start_at(story: Story, scene: SceneId) -> Self {
        let state = GameState {
            scene,
            event: None,
            vars: story.initial_vars(),
            choices: vec![],
            backlog: vec![],
            stage: Stage::default(),
            outfit: BTreeMap::new(),
            unlocked: story.wardrobe.items.iter().filter(|i| i.unlocked).map(|i| i.id.clone()).collect(),
            seen: BTreeSet::new(),
        };
        let mut rt = Self::resume(story, state);
        rt.run(false);
        rt
    }

    pub(crate) fn resume(story: Story, state: GameState) -> Self {
        Runtime { story, state, idx: 0, pending: Pending::Ended(String::new()), cues: vec![] }
    }

    pub fn pending(&self) -> &Pending {
        &self.pending
    }

    pub fn current_event(&self) -> Option<&Event> {
        self.story.scene(&self.state.scene)?.events.get(self.idx)
    }

    pub fn take_cues(&mut self) -> Vec<Cue> {
        std::mem::take(&mut self.cues)
    }

    /// Варианты текущего выбора, доступные игроку.
    pub fn options(&self) -> Vec<&ChoiceOption> {
        match (&self.pending, self.current_event().map(|e| &e.kind)) {
            (Pending::Choice, Some(EventKind::Choice { options, .. })) => available(options, &self.state.vars),
            _ => vec![],
        }
    }

    /// Игрок прочитал текст / истекла пауза / закрыт гардероб.
    pub fn advance(&mut self) {
        match self.pending {
            Pending::Delay(_) => self.run(true),
            Pending::Text => {
                if let Some(id) = self.state.event.clone() {
                    self.state.seen.insert(id);
                }
                self.idx += 1;
                self.run(false);
            }
            Pending::Wait(_) | Pending::Wardrobe(_) => {
                self.idx += 1;
                self.run(false);
            }
            Pending::Choice | Pending::Ended(_) | Pending::Error(_) => {}
        }
    }

    /// Игрок выбрал вариант. `false`, если такого варианта сейчас нет.
    pub fn choose(&mut self, option: &OptionId) -> bool {
        let Some(opt) = self.options().into_iter().find(|o| &o.id == option).cloned() else { return false };
        let Some(event) = self.state.event.clone() else { return false };
        for op in &opt.effects {
            op.apply(&mut self.state.vars);
        }
        self.state.choices.push(ChoiceRecord { scene: self.state.scene.clone(), event, option: opt.id, text: opt.text });
        match opt.goto {
            Some(scene) => self.jump(scene),
            None => self.idx += 1,
        }
        self.run(false);
        true
    }

    /// Надеть предмет (или снять, если `item = None`). `false`, если предмет не открыт или не из этого слота.
    pub fn equip(&mut self, slot: &SlotId, item: Option<&ItemId>) -> bool {
        match item {
            None => {
                self.state.outfit.remove(slot);
                true
            }
            Some(id) => {
                let ok = self.state.unlocked.contains(id) && self.story.item(id).is_some_and(|i| &i.slot == slot);
                if ok {
                    self.state.outfit.insert(slot.clone(), id.clone());
                }
                ok
            }
        }
    }

    fn jump(&mut self, scene: SceneId) {
        self.state.scene = scene;
        self.state.event = None;
        self.idx = 0;
    }

    /// Выполнять события, пока одно из них не остановит игру.
    pub(crate) fn run(&mut self, mut skip_delay: bool) {
        for _ in 0..STEP_LIMIT {
            let Some(scene) = self.story.scene(&self.state.scene) else {
                self.pending = Pending::Error(format!("Сцена «{}» не найдена", self.state.scene));
                return;
            };
            let Some(ev) = scene.events.get(self.idx).cloned() else {
                self.pending = Pending::Ended(String::new());
                return;
            };
            self.state.event = Some(ev.id.clone());
            if ev.delay > 0.0 && !skip_delay {
                self.pending = Pending::Delay(ev.delay);
                return;
            }
            skip_delay = false;
            if let Some(p) = self.execute(&ev, true) {
                self.pending = p;
                return;
            }
        }
        self.pending = Pending::Error("Сюжет зациклился: переходы идут по кругу без текста и выборов".into());
    }

    /// Выполнить событие. `Some` — игра остановилась на нём; `fresh = false` при загрузке сохранения.
    pub(crate) fn execute(&mut self, ev: &Event, fresh: bool) -> Option<Pending> {
        self.state.stage.apply(&ev.kind);
        match &ev.kind {
            EventKind::Say { speaker, text, .. } => {
                if fresh {
                    let speaker = speaker.as_ref().and_then(|id| self.story.character(id)).map(|c| c.name.clone()).unwrap_or_default();
                    let text = crate::view::plain_text(text, &self.state.vars);
                    self.state.backlog.push(LogLine { speaker, text });
                    if self.state.backlog.len() > BACKLOG_LIMIT {
                        self.state.backlog.remove(0);
                    }
                }
                return Some(Pending::Text);
            }
            EventKind::Choice { options, .. } => {
                if !available(options, &self.state.vars).is_empty() {
                    return Some(Pending::Choice);
                }
            }
            EventKind::Wait { seconds } => return Some(Pending::Wait(*seconds)),
            EventKind::OpenWardrobe { slots } => return Some(Pending::Wardrobe(slots.clone())),
            EventKind::End { title } => return Some(Pending::Ended(title.clone())),
            EventKind::Goto { scene } => {
                self.jump(scene.clone());
                return None;
            }
            EventKind::Branch { cond, scene } => {
                if cond.eval(&self.state.vars) {
                    self.jump(scene.clone());
                    return None;
                }
            }
            EventKind::SetVar(op) => op.apply(&mut self.state.vars),
            EventKind::SetOutfit { slot, item } => {
                match item {
                    Some(i) => self.state.outfit.insert(slot.clone(), i.clone()),
                    None => self.state.outfit.remove(slot),
                };
            }
            EventKind::UnlockItem { item } => {
                self.state.unlocked.insert(item.clone());
            }
            EventKind::Sound { file, volume } => self.cues.push(Cue { sound: file.clone(), volume: *volume }),
            _ => {}
        }
        self.idx += 1;
        None
    }
}

fn available<'a>(options: &'a [ChoiceOption], vars: &Vars) -> Vec<&'a ChoiceOption> {
    options.iter().filter(|o| o.cond.as_ref().is_none_or(|c| c.eval(vars))).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::cond::Cond;

    pub fn say(text: &str) -> Event {
        Event::new(EventKind::Say { speaker: None, style: TextStyle::Narration, text: text.into(), expression: None, speed: None, sound: None, look: TextLook::default() })
    }

    pub fn scene(id: &str, events: Vec<Event>) -> Scene {
        Scene { id: id.into(), name: id.into(), description: String::new(), chapter: None, events }
    }

    pub fn story(scenes: Vec<Scene>) -> Story {
        let mut s = Story::new("Тест");
        s.meta.id = "story_test".into();
        s.meta.start_scene = scenes.first().map(|s| s.id.clone());
        s.scenes = scenes;
        s
    }

    fn text(rt: &Runtime) -> String {
        match rt.current_event().map(|e| &e.kind) {
            Some(EventKind::Say { text, .. }) if rt.pending == Pending::Text => text.clone(),
            other => panic!("ожидался текст, а там {:?} / {other:?}", rt.pending),
        }
    }

    /// История с развилкой: помощь доступна всегда, «подкупить» — только при деньгах.
    pub fn branching() -> Story {
        let mut choice = vec![ChoiceOption::new("Помочь", Some("help".into())), ChoiceOption::new("Подкупить", Some("bribe".into())), ChoiceOption::new("Промолчать", None)];
        choice[0].id = "op_help".into();
        choice[0].effects = vec![VarOp { var: "trust".into(), action: VarAction::Add, value: Value::Int(2) }];
        choice[1].id = "op_bribe".into();
        choice[1].cond = Some(Cond::parse("money >= 100").unwrap());
        choice[2].id = "op_silent".into();
        let mut s = story(vec![
            scene("start", vec![say("Привет"), Event::new(EventKind::Choice { prompt: "Что делать?".into(), options: choice }), say("Тишина"), Event::new(EventKind::Branch { cond: Cond::parse("trust > 0").unwrap(), scene: "help".into() }), Event::new(EventKind::End { title: "Одиночество".into() })]),
            scene("help", vec![say("Спасибо"), Event::new(EventKind::End { title: "Дружба".into() })]),
            scene("bribe", vec![say("Деньги взяты"), Event::new(EventKind::End { title: "Сделка".into() })]),
        ]);
        s.variables = vec![VarDef { name: "trust".into(), initial: Value::Int(0), description: String::new() }, VarDef { name: "money".into(), initial: Value::Int(50), description: String::new() }];
        s
    }

    #[test]
    fn linear_scene_runs_to_its_ending() {
        let mut rt = Runtime::new(story(vec![scene("a", vec![say("раз"), say("два"), Event::new(EventKind::End { title: "Конец".into() })])]));
        assert_eq!(text(&rt), "раз");
        rt.advance();
        assert_eq!(text(&rt), "два");
        rt.advance();
        assert_eq!(rt.pending, Pending::Ended("Конец".into()));
        assert_eq!(rt.state.backlog.len(), 2);
    }

    #[test]
    fn choice_hides_options_whose_condition_fails_and_applies_effects() {
        let mut rt = Runtime::new(branching());
        rt.advance();
        assert_eq!(rt.options().iter().map(|o| o.text.as_str()).collect::<Vec<_>>(), vec!["Помочь", "Промолчать"]);
        assert!(!rt.choose(&"op_bribe".into()), "скрытый вариант выбрать нельзя");
        assert!(rt.choose(&"op_help".into()));
        assert_eq!(rt.state.vars["trust"], Value::Int(2));
        assert_eq!(text(&rt), "Спасибо");
        assert_eq!(rt.state.choices[0].text, "Помочь");
    }

    #[test]
    fn option_without_target_continues_scene_and_branch_falls_through() {
        let mut rt = Runtime::new(branching());
        rt.advance();
        rt.choose(&"op_silent".into());
        assert_eq!(text(&rt), "Тишина");
        rt.advance();
        assert_eq!(rt.pending, Pending::Ended("Одиночество".into()));
    }

    #[test]
    fn delay_waits_before_the_event_and_then_runs_it() {
        let mut second = say("позже");
        second.delay = 0.5;
        let mut rt = Runtime::new(story(vec![scene("a", vec![say("сразу"), second])]));
        rt.advance();
        assert_eq!(rt.pending, Pending::Delay(0.5));
        rt.advance();
        assert_eq!(text(&rt), "позже");
    }

    #[test]
    fn scene_without_ending_stops_instead_of_crashing() {
        let mut rt = Runtime::new(story(vec![scene("a", vec![say("всё")])]));
        rt.advance();
        assert_eq!(rt.pending, Pending::Ended(String::new()));
    }

    #[test]
    fn missing_scene_and_endless_loop_are_errors_not_hangs() {
        let rt = Runtime::new(story(vec![scene("a", vec![Event::new(EventKind::Goto { scene: "nowhere".into() })])]));
        assert!(matches!(rt.pending, Pending::Error(_)));
        let rt = Runtime::new(story(vec![scene("a", vec![Event::new(EventKind::Goto { scene: "a".into() })])]));
        assert!(matches!(rt.pending, Pending::Error(_)));
    }

    #[test]
    fn stage_follows_show_move_hide_and_music() {
        let ann: CharId = "ann".into();
        let sc = scene(
            "a",
            vec![
                Event::new(EventKind::Background { image: "bg.png".into(), fade: 0.0 }),
                Event::new(EventKind::PlayMusic { track: "calm.ogg".into(), volume: 0.8, fade: 1.0, looped: true }),
                Event::new(EventKind::ShowCharacter { character: ann.clone(), expression: "smile".into(), pos: Pos::LEFT, fade: 0.3 }),
                Event::new(EventKind::MoveCharacter { character: ann.clone(), pos: Pos::RIGHT, duration: 1.0 }),
                Event::new(EventKind::Expression { character: ann.clone(), expression: "sad".into() }),
                Event::new(EventKind::HideCharacter { character: ann, fade: 0.2 }),
                Event::new(EventKind::StopMusic { fade: 1.0 }),
            ],
        );
        let st = stage_at(&sc, 4);
        assert_eq!(st.background.as_deref(), Some("bg.png"));
        assert_eq!(st.music.as_ref().unwrap().track, "calm.ogg");
        assert_eq!((st.characters[0].pos, st.characters[0].expression.as_str()), (Pos::RIGHT, "sad"));
        let st = stage_at(&sc, 6);
        assert!(st.characters.is_empty() && st.music.is_none());
    }

    #[test]
    fn wardrobe_only_equips_unlocked_items_of_the_slot() {
        let mut s = story(vec![scene("a", vec![Event::new(EventKind::OpenWardrobe { slots: vec![] }), Event::new(EventKind::UnlockItem { item: "coat".into() }), say("готово")])]);
        s.wardrobe.slots = vec![Slot { id: "top".into(), name: "Верх".into(), behind: false }];
        s.wardrobe.items = vec![Item { id: "shirt".into(), slot: "top".into(), name: "Рубашка".into(), image: "s.png".into(), unlocked: true }, Item { id: "coat".into(), slot: "top".into(), name: "Пальто".into(), image: "c.png".into(), unlocked: false }];
        let mut rt = Runtime::new(s);
        assert_eq!(rt.pending, Pending::Wardrobe(vec![]));
        assert!(rt.equip(&"top".into(), Some(&"shirt".into())));
        assert!(!rt.equip(&"top".into(), Some(&"coat".into())), "закрытый предмет");
        assert!(!rt.equip(&"hat".into(), Some(&"shirt".into())), "чужой слот");
        rt.advance();
        assert!(rt.equip(&"top".into(), Some(&"coat".into())), "после события «открыть предмет»");
        assert_eq!(rt.state.outfit[&SlotId::from("top")], ItemId::from("coat"));
    }

    #[test]
    fn sound_events_become_cues_once() {
        let mut rt = Runtime::new(story(vec![scene("a", vec![Event::new(EventKind::Sound { file: "door.wav".into(), volume: 1.0 }), say("стук")])]));
        assert_eq!(rt.take_cues(), vec![Cue { sound: "door.wav".into(), volume: 1.0 }]);
        assert!(rt.take_cues().is_empty());
    }
}
