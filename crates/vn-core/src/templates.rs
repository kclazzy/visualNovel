//! Заготовки событий и копирование сцен/глав: автору не нужно собирать типовое с нуля.
use crate::model::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    Dialogue,
    Entrance,
    BackgroundChange,
    PlayerChoice,
    Transition,
    Romantic,
    Battle,
    SystemMessage,
}

fn say(speaker: Option<CharId>, style: TextStyle, text: &str) -> Event {
    Event::new(EventKind::Say { speaker, style, text: text.into(), expression: None, speed: None, sound: None, look: TextLook::default() })
}

impl Template {
    pub const ALL: [Template; 8] = [Template::Dialogue, Template::Entrance, Template::BackgroundChange, Template::PlayerChoice, Template::Transition, Template::Romantic, Template::Battle, Template::SystemMessage];

    pub fn name(self) -> &'static str {
        match self {
            Template::Dialogue => "Диалог персонажей",
            Template::Entrance => "Появление персонажа",
            Template::BackgroundChange => "Смена фона",
            Template::PlayerChoice => "Выбор игрока",
            Template::Transition => "Переход между сценами",
            Template::Romantic => "Романтическая сцена",
            Template::Battle => "Боевая сцена",
            Template::SystemMessage => "Системное сообщение",
        }
    }

    /// События заготовки. Персонажи берутся из истории, если они уже есть.
    pub fn events(self, story: &Story) -> Vec<Event> {
        let who = |n: usize| story.characters.get(n).map(|c| c.id.clone());
        let show = |n: usize, pos: Pos| who(n).map(|character| Event::new(EventKind::ShowCharacter { character, expression: String::new(), pos, fade: 0.3 }));
        let fade = |out: bool| Event::new(EventKind::Fade { out, duration: 0.5 });
        let two_options = || Event::new(EventKind::Choice { prompt: String::new(), options: vec![ChoiceOption::new("Первый вариант", None), ChoiceOption::new("Второй вариант", None)] });
        match self {
            Template::Dialogue => vec![say(who(0), TextStyle::Dialogue, "Первая реплика."), say(who(1), TextStyle::Dialogue, "Ответ."), say(who(0), TextStyle::Dialogue, "Ещё реплика.")],
            Template::Entrance => show(0, Pos::CENTER).into_iter().chain([say(who(0), TextStyle::Dialogue, "Привет!")]).collect(),
            Template::BackgroundChange => vec![fade(true), Event::new(EventKind::Background { image: String::new(), fade: 0.0 }), fade(false)],
            Template::PlayerChoice => vec![say(None, TextStyle::Narration, "Что делать?"), two_options()],
            Template::Transition => vec![fade(true), Event::new(EventKind::StopMusic { fade: 0.5 }), Event::new(EventKind::Goto { scene: story.meta.start_scene.clone().unwrap_or_default() })],
            Template::Romantic => [Some(Event::new(EventKind::PlayMusic { track: String::new(), volume: 0.6, fade: 1.5, looped: true })), show(0, Pos::CENTER)]
                .into_iter()
                .flatten()
                .chain([say(who(0), TextStyle::Dialogue, "Я давно хотела тебе сказать…"), say(None, TextStyle::Thought, "Сердце забилось быстрее."), two_options()])
                .collect(),
            Template::Battle => [Some(Event::new(EventKind::PlayMusic { track: String::new(), volume: 0.9, fade: 0.2, looped: true })), show(0, Pos::LEFT), show(1, Pos::RIGHT)]
                .into_iter()
                .flatten()
                .chain([Event::new(EventKind::Sound { file: String::new(), volume: 1.0 }), say(None, TextStyle::Narration, "Противник бросается вперёд!"), two_options()])
                .collect(),
            Template::SystemMessage => vec![say(None, TextStyle::System, "Получен новый предмет.")],
        }
    }
}

fn fresh_events(events: &[Event], remap: &BTreeMap<SceneId, SceneId>) -> Vec<Event> {
    let target = |s: &mut SceneId| {
        if let Some(n) = remap.get(s) {
            *s = n.clone();
        }
    };
    events
        .iter()
        .cloned()
        .map(|mut e| {
            e.id = EventId::new("ev");
            match &mut e.kind {
                EventKind::Goto { scene } | EventKind::Branch { scene, .. } => target(scene),
                EventKind::Choice { options, .. } => {
                    for o in options {
                        o.id = OptionId::new("op");
                        if let Some(s) = &mut o.goto {
                            target(s);
                        }
                    }
                }
                _ => {}
            }
            e
        })
        .collect()
}

fn copy_scenes(story: &mut Story, ids: &[SceneId], chapter: Option<Option<ChapterId>>) -> Vec<SceneId> {
    let remap: BTreeMap<SceneId, SceneId> = ids.iter().map(|id| (id.clone(), SceneId::new("sc"))).collect();
    let copies: Vec<Scene> = story
        .scenes
        .iter()
        .filter(|s| ids.contains(&s.id))
        .map(|s| Scene { id: remap[&s.id].clone(), name: format!("{} (копия)", s.name), description: s.description.clone(), chapter: chapter.clone().unwrap_or(s.chapter.clone()), events: fresh_events(&s.events, if ids.len() > 1 { &remap } else { &EMPTY }) })
        .collect();
    for (old, new) in &remap {
        if let Some([x, y]) = story.layout.get(old).copied() {
            story.layout.insert(new.clone(), [x + 40.0, y + 60.0]);
        }
    }
    let out = copies.iter().map(|s| s.id.clone()).collect();
    story.scenes.extend(copies);
    out
}

static EMPTY: BTreeMap<SceneId, SceneId> = BTreeMap::new();

/// Копия сцены с новыми ID. Переходы ведут туда же, куда и в оригинале.
pub fn duplicate_scene(story: &mut Story, id: &SceneId) -> Option<SceneId> {
    copy_scenes(story, std::slice::from_ref(id), None).pop()
}

/// Копия главы со всеми сценами. Переходы внутри главы ведут в копии сцен.
pub fn duplicate_chapter(story: &mut Story, id: &ChapterId) -> Option<ChapterId> {
    let source = story.chapters.iter().find(|c| &c.id == id)?.clone();
    let copy = Chapter { id: ChapterId::new("chp"), name: format!("{} (копия)", source.name) };
    let ids: Vec<SceneId> = story.scenes.iter().filter(|s| s.chapter.as_ref() == Some(id)).map(|s| s.id.clone()).collect();
    copy_scenes(story, &ids, Some(Some(copy.id.clone())));
    story.chapters.push(copy.clone());
    Some(copy.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::tests::branching;
    use std::collections::BTreeSet;

    #[test]
    fn every_template_produces_events_with_unique_ids() {
        let mut story = branching();
        story.characters = vec![Character::new("Анна"), Character::new("Макс")];
        let mut ids = BTreeSet::new();
        for t in Template::ALL {
            let events = t.events(&story);
            assert!(!events.is_empty(), "{}", t.name());
            for e in events {
                assert!(ids.insert(e.id), "повтор ID в «{}»", t.name());
            }
        }
        assert!(!Template::Entrance.events(&branching()).is_empty(), "работает и без персонажей");
    }

    #[test]
    fn duplicated_scene_gets_new_ids_but_keeps_targets() {
        let mut story = branching();
        let copy = duplicate_scene(&mut story, &"start".into()).unwrap();
        let (orig, dup) = (story.scene(&"start".into()).unwrap(), story.scene(&copy).unwrap());
        assert_eq!(dup.name, "start (копия)");
        assert_eq!(dup.events.len(), orig.events.len());
        assert!(dup.events.iter().zip(&orig.events).all(|(a, b)| a.id != b.id));
        let targets = |sc: &SceneId| story.edges().into_iter().filter(|e| &e.from == sc).map(|e| e.to).collect::<Vec<_>>();
        assert_eq!(targets(&copy), targets(&"start".into()));
        assert!(duplicate_scene(&mut story, &"nope".into()).is_none());
    }

    #[test]
    fn duplicated_chapter_rewires_internal_links_to_the_copies() {
        let mut story = branching();
        story.chapters = vec![Chapter { id: "c1".into(), name: "Глава 1".into() }];
        for s in &mut story.scenes[..2] {
            s.chapter = Some("c1".into());
        }
        let new = duplicate_chapter(&mut story, &"c1".into()).unwrap();
        let copies: Vec<&Scene> = story.scenes.iter().filter(|s| s.chapter.as_ref() == Some(&new)).collect();
        assert_eq!(copies.len(), 2);
        let edges = story.edges();
        let from_copy: Vec<&Edge> = edges.iter().filter(|e| e.from == copies[0].id).collect();
        assert!(from_copy.iter().any(|e| e.to == copies[1].id), "ссылка на help ведёт в копию help");
        assert!(from_copy.iter().any(|e| e.to.as_str() == "bribe"), "ссылка наружу не тронута");
    }
}
