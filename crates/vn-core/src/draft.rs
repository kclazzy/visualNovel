//! Черновик истории от ИИ: строгий JSON, который проверяется на входе и превращается в сцены.
//! ИИ не видит и не выдумывает ID — их назначает редактор.
use crate::model::*;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct DraftCharacter {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct DraftLine {
    #[serde(default)]
    pub speaker: Option<String>,
    pub text: String,
    #[serde(default)]
    pub thought: bool,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct DraftChoice {
    pub text: String,
    pub goto: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct DraftScene {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub chapter: Option<String>,
    #[serde(default)]
    pub lines: Vec<DraftLine>,
    #[serde(default)]
    pub choices: Vec<DraftChoice>,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub ending: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Draft {
    #[serde(default)]
    pub characters: Vec<DraftCharacter>,
    pub scenes: Vec<DraftScene>,
}

pub const SYSTEM_PROMPT: &str = r#"Ты сценарист визуальных новелл. Отвечай ТОЛЬКО одним JSON-объектом без пояснений и без markdown.
Формат:
{
  "characters": [{"name": "Имя", "description": "кратко о персонаже"}],
  "scenes": [{
    "key": "короткий_латинский_ключ",
    "name": "Название сцены",
    "chapter": "Название главы",
    "lines": [
      {"speaker": "Имя", "text": "Реплика персонажа"},
      {"speaker": "Имя", "text": "Мысли персонажа", "thought": true},
      {"text": "Авторское повествование"}
    ],
    "choices": [{"text": "Вариант игрока", "goto": "ключ_сцены"}],
    "next": "ключ_сцены",
    "ending": "Название концовки"
  }]
}
Правила: у каждой сцены ровно одно из полей choices, next или ending. Все goto и next указывают на key существующих сцен из этого же ответа. Первая сцена — начало истории. Не используй поля, которых нет в формате."#;

/// Запрос на новую историю.
pub fn story_prompt(idea: &str, scenes: u32, language: &str) -> String {
    format!("Придумай визуальную новеллу. Язык текста: {language}. Сцен: около {scenes}. В истории должно быть минимум два выбора игрока и минимум две разные концовки. В каждой сцене 4–8 реплик.\n\nИдея автора: {idea}")
}

/// Запрос на продолжение сцены: модель видит персонажей и последние реплики.
pub fn continue_prompt(story: &Story, scene: &Scene, wish: &str, language: &str) -> String {
    let cast: Vec<String> = story.characters.iter().map(|c| format!("{} — {}", c.name, c.description)).collect();
    let mut lines: Vec<String> = scene
        .events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::Say { speaker, text, .. } => Some(match speaker.as_ref().and_then(|s| story.character(s)) {
                Some(c) => format!("{}: {text}", c.name),
                None => text.clone(),
            }),
            _ => None,
        })
        .collect();
    let tail = lines.split_off(lines.len().saturating_sub(20));
    format!(
        "Продолжи сцену визуальной новеллы «{}». Язык текста: {language}. Верни JSON с одной сценой (key \"continue\"), в lines — только НОВЫЕ 5–8 реплик, поля choices/next/ending не заполняй. Используй только этих персонажей.\n\nПерсонажи:\n{}\n\nСцена «{}», последние реплики:\n{}\n\nПожелание автора: {}",
        story.meta.name,
        cast.join("\n"),
        scene.name,
        tail.join("\n"),
        if wish.trim().is_empty() { "продолжай естественно" } else { wish }
    )
}

/// Разобрать ответ модели. Терпит обёртку ```json и текст вокруг объекта.
pub fn parse_draft(reply: &str) -> Result<Draft, String> {
    let (Some(start), Some(end)) = (reply.find('{'), reply.rfind('}')) else { return Err("В ответе ИИ нет JSON-объекта. Попробуйте ещё раз или выберите другую модель.".into()) };
    let draft: Draft = serde_json::from_str(&reply[start..=end]).map_err(|e| format!("Ответ ИИ не соответствует формату: {e}"))?;
    if draft.scenes.is_empty() {
        return Err("ИИ не вернул ни одной сцены.".into());
    }
    let mut keys = BTreeSet::new();
    for s in &draft.scenes {
        if s.key.trim().is_empty() || !keys.insert(s.key.as_str()) {
            return Err(format!("У сцен ИИ пустой или повторяющийся ключ «{}».", s.key));
        }
    }
    Ok(draft)
}

const PALETTE: [[u8; 3]; 6] = [[255, 170, 190], [140, 200, 255], [170, 230, 160], [255, 215, 120], [200, 170, 255], [255, 160, 120]];

fn character_id(story: &mut Story, name: &str, description: &str) -> CharId {
    if let Some(c) = story.characters.iter().find(|c| c.name.to_lowercase() == name.trim().to_lowercase()) {
        return c.id.clone();
    }
    let mut c = Character::new(name.trim());
    c.description = description.to_string();
    c.color = PALETTE[story.characters.len() % PALETTE.len()];
    story.characters.push(c.clone());
    c.id
}

fn say_events(story: &mut Story, lines: &[DraftLine]) -> Vec<Event> {
    lines
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .map(|l| {
            let speaker = l.speaker.as_deref().filter(|s| !s.trim().is_empty()).map(|s| character_id(story, s, ""));
            let style = match (&speaker, l.thought) {
                (_, true) => TextStyle::Thought,
                (Some(_), false) => TextStyle::Dialogue,
                (None, false) => TextStyle::Narration,
            };
            Event::new(EventKind::Say { speaker, style, text: l.text.trim().to_string(), expression: None, speed: None, sound: None })
        })
        .collect()
}

/// Добавить сцены черновика в историю. Возвращает ID новых сцен.
/// Ссылки на неизвестные ключи остаются пустыми — их покажет проверка проекта.
pub fn apply_draft(story: &mut Story, draft: &Draft) -> Vec<SceneId> {
    for c in &draft.characters {
        character_id(story, &c.name, &c.description);
    }
    let pristine = story.scenes.len() == 1 && story.scenes[0].events.is_empty();
    if pristine {
        story.scenes.clear();
        story.layout.clear();
    }
    let ids: BTreeMap<&str, SceneId> = draft.scenes.iter().map(|s| (s.key.as_str(), SceneId::new("sc"))).collect();
    let floor = story.layout.values().map(|p| p[1]).fold(-180.0f32, f32::max) + 180.0;
    for (i, d) in draft.scenes.iter().enumerate() {
        let chapter = d.chapter.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(|name| match story.chapters.iter().find(|c| c.name == name) {
            Some(c) => c.id.clone(),
            None => {
                let c = Chapter { id: ChapterId::new("chp"), name: name.to_string() };
                story.chapters.push(c.clone());
                c.id
            }
        });
        let mut events = say_events(story, &d.lines);
        let target = |key: &str| ids.get(key).cloned();
        if !d.choices.is_empty() {
            events.push(Event::new(EventKind::Choice { prompt: String::new(), options: d.choices.iter().map(|c| ChoiceOption::new(c.text.trim(), target(&c.goto))).collect() }));
        } else if let Some(to) = d.next.as_deref().and_then(target) {
            events.push(Event::new(EventKind::Goto { scene: to }));
        } else if let Some(title) = &d.ending {
            events.push(Event::new(EventKind::End { title: title.clone() }));
        }
        let id = ids[d.key.as_str()].clone();
        story.layout.insert(id.clone(), [(i % 4) as f32 * 260.0, floor + (i / 4) as f32 * 170.0]);
        story.scenes.push(Scene { id, name: d.name.clone(), description: String::new(), chapter, events });
    }
    let new: Vec<SceneId> = draft.scenes.iter().map(|d| ids[d.key.as_str()].clone()).collect();
    if pristine || story.meta.start_scene.is_none() {
        story.meta.start_scene = new.first().cloned();
    }
    new
}

/// Дописать реплики первой сцены черновика в существующую сцену — перед её финальным переходом/выбором.
/// Возвращает число добавленных реплик.
pub fn append_lines(story: &mut Story, scene: &SceneId, draft: &Draft) -> usize {
    let Some(lines) = draft.scenes.first().map(|s| s.lines.clone()) else { return 0 };
    let events = say_events(story, &lines);
    let Some(sc) = story.scene_mut(scene) else { return 0 };
    let at = match sc.events.last().map(|e| &e.kind) {
        Some(k) if k.is_terminal() || matches!(k, EventKind::Choice { .. }) => sc.events.len() - 1,
        _ => sc.events.len(),
    };
    let n = events.len();
    sc.events.splice(at..at, events);
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Pending, Runtime};
    use crate::validate::{has_errors, validate};

    const REPLY: &str = r#"Вот история:
```json
{"characters":[{"name":"Анна","description":"подруга детства"}],
 "scenes":[
  {"key":"meet","name":"Встреча","chapter":"Глава 1","lines":[{"speaker":"Анна","text":"Привет!"},{"text":"Она улыбнулась."},{"speaker":"анна","text":"Опять он…","thought":true}],
   "choices":[{"text":"Помочь","goto":"help"},{"text":"Уйти","goto":"leave"}]},
  {"key":"help","name":"Помощь","chapter":"Глава 1","lines":[{"speaker":"Макс","text":"Я помогу."}],"next":"leave"},
  {"key":"leave","name":"Прощание","lines":[{"text":"Конец дня."}],"ending":"Обычный день"}
 ]}
```
Надеюсь, понравится!"#;

    #[test]
    fn reply_with_fences_becomes_a_playable_valid_story() {
        let draft = parse_draft(REPLY).unwrap();
        let mut story = Story::new("ИИ");
        let new = apply_draft(&mut story, &draft);
        assert_eq!(new.len(), 3);
        assert_eq!(story.scenes.len(), 3, "пустая стартовая сцена заменена");
        assert_eq!(story.meta.start_scene, Some(new[0].clone()));
        assert_eq!(story.characters.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), vec!["Анна", "Макс"], "«анна» — тот же персонаж");
        assert_eq!(story.chapters.len(), 1);
        assert!(!has_errors(&validate(&story, &BTreeSet::new())));

        let mut rt = Runtime::new(story);
        rt.advance();
        rt.advance();
        assert!(matches!(rt.current_event().unwrap().kind, EventKind::Say { style: TextStyle::Thought, .. }));
        rt.advance();
        let help = rt.options()[0].id.clone();
        rt.choose(&help);
        rt.advance();
        rt.advance();
        assert_eq!(*rt.pending(), Pending::Ended("Обычный день".into()));
    }

    #[test]
    fn existing_story_is_extended_not_replaced() {
        let mut story = crate::runtime::tests::branching();
        let before = story.scenes.len();
        let start = story.meta.start_scene.clone();
        apply_draft(&mut story, &parse_draft(REPLY).unwrap());
        assert_eq!(story.scenes.len(), before + 3);
        assert_eq!(story.meta.start_scene, start);
    }

    #[test]
    fn bad_replies_are_explained() {
        assert!(parse_draft("Извините, не могу.").unwrap_err().contains("нет JSON"));
        assert!(parse_draft(r#"{"scenes":[]}"#).unwrap_err().contains("ни одной сцены"));
        assert!(parse_draft(r#"{"scenes":[{"key":"a","name":"A"},{"key":"a","name":"B"}]}"#).unwrap_err().contains("повторяющийся ключ"));
        assert!(parse_draft(r#"{"scenes":[{"name":"A"}]}"#).unwrap_err().contains("не соответствует формату"));
    }

    #[test]
    fn unknown_target_is_left_for_the_validator() {
        let draft = parse_draft(r#"{"scenes":[{"key":"a","name":"A","lines":[{"text":"x"}],"next":"ghost"}]}"#).unwrap();
        let mut story = Story::new("t");
        apply_draft(&mut story, &draft);
        assert!(has_errors(&validate(&story, &BTreeSet::new())), "сцена обрывается — автор увидит ошибку");
    }

    #[test]
    fn continuation_goes_before_the_final_choice() {
        let mut story = crate::runtime::tests::branching();
        let draft = parse_draft(r#"{"scenes":[{"key":"continue","name":"","lines":[{"text":"Новая 1"},{"text":"  "},{"speaker":"Гость","text":"Новая 2"}]}]}"#).unwrap();
        let help: SceneId = "help".into();
        assert_eq!(append_lines(&mut story, &help, &draft), 2);
        let sc = story.scene(&help).unwrap();
        assert_eq!(sc.events.len(), 4);
        assert!(matches!(sc.events[3].kind, EventKind::End { .. }));
        assert!(matches!(&sc.events[2].kind, EventKind::Say { speaker: Some(_), .. }));
        let prompt = continue_prompt(&story, story.scene(&help).unwrap(), "", "русский");
        assert!(prompt.contains("Гость: Новая 2") && prompt.contains("Спасибо"));
    }
}
