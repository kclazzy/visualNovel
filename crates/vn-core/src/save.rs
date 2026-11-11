//! Сохранения игрока и их перенос на обновлённую историю.
//! Положение хранится как ID сцены и события, поэтому переживает правку сценария.
use crate::model::*;
use crate::runtime::{GameState, Runtime};
use serde::{Deserialize, Serialize};

/// Версия формата сохранения. При изменении `GameState` добавляется шаг в `MIGRATIONS`.
pub const SAVE_VERSION: u32 = 1;

/// Шаг `i` переводит сохранение версии `i + 1` в версию `i + 2`.
const MIGRATIONS: &[fn(&mut serde_json::Value)] = &[];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SaveFile {
    pub save_version: u32,
    pub story_id: String,
    pub story_version: u32,
    /// Секунды Unix.
    pub saved_at: u64,
    pub label: String,
    pub state: GameState,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SaveError {
    Corrupt(String),
    /// Сохранение сделано более новой версией игры.
    TooNew(u32),
    WrongStory,
    /// Сцена сохранения удалена, и автор не указал, куда отправить игрока.
    Incompatible,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Corrupt(e) => write!(f, "Файл сохранения повреждён: {e}"),
            SaveError::TooNew(v) => write!(f, "Сохранение сделано более новой версией игры (формат {v}). Обновите игру."),
            SaveError::WrongStory => f.write_str("Это сохранение от другой истории."),
            SaveError::Incompatible => f.write_str("Сохранение несовместимо с этой версией истории: сцена, в которой оно сделано, удалена."),
        }
    }
}

/// Как игра продолжилась после загрузки.
#[derive(Clone, Debug, PartialEq)]
pub enum Restored {
    /// С того же события.
    Exact,
    /// Событие исчезло — с начала той же сцены.
    SceneRestart,
    /// Сцена удалена — с указанной автором сцены.
    Redirected(SceneId),
}

fn migrate(mut value: serde_json::Value, migrations: &[fn(&mut serde_json::Value)]) -> Result<serde_json::Value, SaveError> {
    let latest = migrations.len() as u32 + 1;
    let version = value.get("save_version").and_then(|v| v.as_u64()).ok_or_else(|| SaveError::Corrupt("нет номера версии".into()))? as u32;
    if version == 0 {
        return Err(SaveError::Corrupt("версия 0".into()));
    }
    if version > latest {
        return Err(SaveError::TooNew(version));
    }
    for step in &migrations[version as usize - 1..] {
        step(&mut value);
    }
    value["save_version"] = latest.into();
    Ok(value)
}

/// Прочитать сохранение любой поддерживаемой версии.
pub fn parse_save(json: &str) -> Result<SaveFile, SaveError> {
    let value = serde_json::from_str(json).map_err(|e| SaveError::Corrupt(e.to_string()))?;
    serde_json::from_value(migrate(value, MIGRATIONS)?).map_err(|e| SaveError::Corrupt(e.to_string()))
}

impl Runtime {
    pub fn save(&self, now: u64) -> SaveFile {
        let scene = self.story.scene(&self.state.scene).map(|s| s.name.as_str()).unwrap_or("");
        let line = self.state.backlog.last().map(|l| l.text.as_str()).unwrap_or("");
        SaveFile { save_version: SAVE_VERSION, story_id: self.story.meta.id.clone(), story_version: self.story.meta.story_version, saved_at: now, label: format!("{scene} — {line}").chars().take(80).collect(), state: self.state.clone() }
    }

    /// Продолжить игру из сохранения на (возможно, обновлённой) истории.
    pub fn restore(story: Story, save: SaveFile) -> Result<(Runtime, Restored), SaveError> {
        if save.story_id != story.meta.id {
            return Err(SaveError::WrongStory);
        }
        let mut state = save.state;
        for (name, value) in story.initial_vars() {
            state.vars.entry(name).or_insert(value);
        }
        state.outfit.retain(|slot, item| story.item(item).is_some_and(|i| &i.slot == slot));
        state.unlocked.retain(|i| story.item(i).is_some());
        state.unlocked.extend(story.wardrobe.items.iter().filter(|i| i.unlocked).map(|i| i.id.clone()));

        let position = story.scene(&state.scene).map(|sc| state.event.as_ref().and_then(|e| sc.event_index(e)));
        let how = match position {
            Some(Some(_)) => Restored::Exact,
            Some(None) => Restored::SceneRestart,
            None => {
                let target = redirect_target(&story, &state.scene).ok_or(SaveError::Incompatible)?;
                state.scene = target.clone();
                state.stage.characters.clear();
                Restored::Redirected(target)
            }
        };
        let mut rt = Runtime::resume(story, state);
        match position.flatten() {
            Some(idx) => {
                rt.idx = idx;
                let ev = rt.current_event().cloned().expect("индекс найден выше");
                match rt.execute(&ev, false) {
                    Some(p) => rt.pending = p,
                    None => rt.run(false),
                }
            }
            None => {
                rt.state.event = None;
                rt.run(false);
            }
        }
        Ok((rt, how))
    }
}

/// Куда ведут перенаправления удалённой сцены; иначе — запасная сцена истории.
fn redirect_target(story: &Story, from: &SceneId) -> Option<SceneId> {
    let mut at = from;
    for _ in 0..story.redirects.len() {
        match story.redirects.get(at) {
            Some(next) if story.scene(next).is_some() => return Some(next.clone()),
            Some(next) => at = next,
            None => break,
        }
    }
    story.meta.fallback_scene.clone().filter(|s| story.scene(s).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::tests::{branching, say, scene};
    use crate::runtime::Pending;

    /// Игрок выбрал «Помочь» и стоит на реплике «Спасибо» в сцене help.
    fn saved() -> (Story, SaveFile) {
        let story = branching();
        let mut rt = Runtime::new(story.clone());
        rt.advance();
        rt.choose(&"op_help".into());
        let json = serde_json::to_string(&rt.save(1000)).unwrap();
        (story, parse_save(&json).unwrap())
    }

    #[test]
    fn save_restores_to_the_same_view_without_duplicating_backlog() {
        let (story, save) = saved();
        let mut original = Runtime::new(story.clone());
        original.advance();
        original.choose(&"op_help".into());
        let (rt, how) = Runtime::restore(story, save).unwrap();
        assert_eq!(how, Restored::Exact);
        assert_eq!(rt.view(), original.view());
        assert_eq!(rt.state, original.state);
    }

    #[test]
    fn story_grown_after_release_keeps_position_and_adds_new_variables() {
        let (mut story, save) = saved();
        story.meta.story_version = 2;
        story.variables.push(VarDef { name: "chapter2".into(), initial: Value::Bool(false), description: String::new() });
        let help = story.scene_mut(&"help".into()).unwrap();
        help.events.insert(0, say("Новая вступительная реплика"));
        help.events.pop();
        help.events.push(Event::new(EventKind::Goto { scene: "next".into() }));
        story.scenes.push(scene("next", vec![say("Продолжение"), Event::new(EventKind::End { title: "Глава 2".into() })]));
        let (mut rt, how) = Runtime::restore(story, save).unwrap();
        assert_eq!(how, Restored::Exact);
        assert_eq!(rt.state.vars["chapter2"], Value::Bool(false));
        assert_eq!(rt.state.vars["trust"], Value::Int(2), "прогресс сохранён");
        assert_eq!(rt.state.backlog.last().unwrap().text, "Спасибо");
        rt.advance();
        assert_eq!(rt.state.backlog.last().unwrap().text, "Продолжение");
    }

    #[test]
    fn removed_event_restarts_the_scene() {
        let (mut story, save) = saved();
        story.scene_mut(&"help".into()).unwrap().events[0] = say("Переписано");
        let (rt, how) = Runtime::restore(story, save).unwrap();
        assert_eq!(how, Restored::SceneRestart);
        assert_eq!(rt.state.backlog.last().unwrap().text, "Переписано");
    }

    #[test]
    fn removed_scene_follows_redirect_then_fallback_then_fails() {
        let (story, save) = saved();
        let mut redirected = story.clone();
        redirected.remove_scene(&"help".into(), Some("bribe".into()));
        let (rt, how) = Runtime::restore(redirected, save.clone()).unwrap();
        assert_eq!(how, Restored::Redirected("bribe".into()));
        assert_eq!(rt.state.backlog.last().unwrap().text, "Деньги взяты");

        let mut fallback = story.clone();
        fallback.remove_scene(&"help".into(), None);
        fallback.meta.fallback_scene = Some("start".into());
        assert_eq!(Runtime::restore(fallback, save.clone()).unwrap().1, Restored::Redirected("start".into()));

        let mut gone = story;
        gone.remove_scene(&"help".into(), None);
        assert_eq!(Runtime::restore(gone, save).unwrap_err(), SaveError::Incompatible);
    }

    #[test]
    fn redirect_chain_is_followed() {
        let (mut story, save) = saved();
        story.remove_scene(&"help".into(), Some("bribe".into()));
        story.remove_scene(&"bribe".into(), Some("start".into()));
        assert_eq!(Runtime::restore(story, save).unwrap().1, Restored::Redirected("start".into()));
    }

    #[test]
    fn foreign_broken_and_future_saves_are_rejected() {
        let (mut story, save) = saved();
        let mut future = serde_json::to_value(&save).unwrap();
        future["save_version"] = 99.into();
        assert_eq!(parse_save(&future.to_string()).unwrap_err(), SaveError::TooNew(99));
        assert!(matches!(parse_save("{не json").unwrap_err(), SaveError::Corrupt(_)));
        assert!(matches!(parse_save("{}").unwrap_err(), SaveError::Corrupt(_)));
        story.meta.id = "other".into();
        assert_eq!(Runtime::restore(story, save).unwrap_err(), SaveError::WrongStory);
    }

    #[test]
    fn migration_steps_run_from_the_saved_version_up() {
        fn add_a(v: &mut serde_json::Value) {
            v["a"] = 1.into();
        }
        fn add_b(v: &mut serde_json::Value) {
            v["b"] = v["a"].clone();
        }
        let steps: &[fn(&mut serde_json::Value)] = &[add_a, add_b];
        let v1 = migrate(serde_json::json!({"save_version": 1}), steps).unwrap();
        assert_eq!(v1, serde_json::json!({"save_version": 3, "a": 1, "b": 1}));
        let v2 = migrate(serde_json::json!({"save_version": 2, "a": 7}), steps).unwrap();
        assert_eq!(v2, serde_json::json!({"save_version": 3, "a": 7, "b": 7}));
    }

    #[test]
    fn deleted_wardrobe_items_are_dropped_on_load() {
        let (mut story, mut save) = saved();
        story.wardrobe.slots = vec![Slot { id: "top".into(), name: "Верх".into(), behind: false }];
        story.wardrobe.items = vec![Item { id: "new".into(), slot: "top".into(), name: "Новое".into(), image: "n.png".into(), unlocked: true }];
        save.state.outfit.insert("top".into(), "old".into());
        save.state.unlocked.insert("old".into());
        let (rt, _) = Runtime::restore(story, save).unwrap();
        assert!(rt.state.outfit.is_empty());
        assert_eq!(rt.state.unlocked.iter().map(|i| i.as_str()).collect::<Vec<_>>(), vec!["new"]);
        assert_eq!(*rt.pending(), Pending::Text);
    }
}
