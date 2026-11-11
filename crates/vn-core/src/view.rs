//! Готовая к показу картина игры. Отрисовщики (окно игры, предпросмотр, веб-плеер)
//! ничего не решают сами — только рисуют `View`.
use crate::model::*;
use crate::runtime::{GameState, Music, Pending, Runtime, Stage};
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Span {
    pub text: String,
    pub bold: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sprite {
    pub id: CharId,
    /// Изображения слоёв снизу вверх.
    pub layers: Vec<String>,
    pub x: f32,
    pub y: f32,
    /// Высота в долях высоты экрана.
    pub height: f32,
    pub fade: f32,
    pub move_time: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Speaker {
    pub name: String,
    pub color: [u8; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OptionView {
    pub id: OptionId,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Prompt {
    /// `color` и `size` уже учитывают оформление реплики; `size` — высота шрифта в долях высоты экрана.
    Text { speaker: Option<Speaker>, style: TextStyle, spans: Vec<Span>, speed: f32, seen: bool, sound: Option<String>, color: [u8; 3], size: f32 },
    Choice { prompt: String, options: Vec<OptionView> },
    Wait { seconds: f32 },
    Wardrobe { slots: Vec<SlotId> },
    Ended { title: String },
    Error { message: String },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct View {
    pub scene: SceneId,
    pub event: Option<EventId>,
    pub background: Option<String>,
    pub bg_fade: f32,
    pub black: bool,
    pub fade: f32,
    pub hide_fade: f32,
    pub sprites: Vec<Sprite>,
    pub music: Option<Music>,
    pub music_fade: f32,
    pub dialog: DialogPos,
    pub prompt: Prompt,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ItemView {
    pub id: ItemId,
    pub name: String,
    pub image: String,
    pub equipped: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SlotView {
    pub id: SlotId,
    pub name: String,
    pub items: Vec<ItemView>,
}

/// Подставить переменные: `Привет, {player_name}!`. Неизвестные имена остаются как есть.
pub fn substitute(text: &str, vars: &Vars) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let Some(len) = rest[open..].find('}') else { break };
        let name = &rest[open + 1..open + len];
        out.push_str(&rest[..open]);
        match vars.get(name) {
            Some(v) => out.push_str(&v.to_string()),
            None => out.push_str(&rest[open..=open + len]),
        }
        rest = &rest[open + len + 1..];
    }
    out + rest
}

/// Текст с подстановкой переменных и выделением `*важных слов*`.
pub fn spans(text: &str, vars: &Vars) -> Vec<Span> {
    substitute(text, vars).split('*').enumerate().filter(|(_, t)| !t.is_empty()).map(|(i, t)| Span { text: t.to_string(), bold: i % 2 == 1 }).collect()
}

pub fn plain_text(text: &str, vars: &Vars) -> String {
    spans(text, vars).into_iter().map(|s| s.text).collect()
}

/// Слои спрайта персонажа: у героя — тело и надетые предметы в порядке слотов.
pub fn sprite_layers(story: &Story, state_outfit: &std::collections::BTreeMap<SlotId, ItemId>, ch: &Character, expression: &str) -> Vec<String> {
    let body = ch.image(expression).map(str::to_string);
    if story.wardrobe.hero.as_ref() != Some(&ch.id) {
        return body.into_iter().collect();
    }
    let worn = |behind: bool| story.wardrobe.slots.iter().filter(move |s| s.behind == behind).filter_map(|s| state_outfit.get(&s.id)).filter_map(|i| story.item(i)).map(|i| i.image.clone());
    worn(true).chain(body.clone()).chain(worn(false)).collect()
}

/// Спрайты сцены. Используется и игрой, и предпросмотром редактора.
pub fn sprites(story: &Story, stage: &Stage, outfit: &std::collections::BTreeMap<SlotId, ItemId>) -> Vec<Sprite> {
    stage
        .characters
        .iter()
        .filter_map(|s| {
            let ch = story.character(&s.character)?;
            Some(Sprite { id: ch.id.clone(), layers: sprite_layers(story, outfit, ch, &s.expression), x: s.pos.x, y: s.pos.y, height: ch.height * s.pos.scale, fade: s.fade, move_time: s.move_time })
        })
        .collect()
}

impl Runtime {
    pub fn view(&self) -> View {
        let st: &GameState = &self.state;
        let prompt = match (self.pending(), self.current_event().map(|e| &e.kind)) {
            (Pending::Text, Some(EventKind::Say { speaker, style, text, speed, sound, look, .. })) => Prompt::Text {
                speaker: speaker.as_ref().and_then(|id| self.story.character(id)).map(|c| Speaker { name: c.name.clone(), color: c.color }),
                style: *style,
                spans: spans(text, &st.vars),
                speed: speed.unwrap_or(self.story.meta.text.speed),
                seen: st.event.as_ref().is_some_and(|e| st.seen.contains(e)),
                sound: sound.clone().filter(|s| !s.is_empty()),
                color: look.color.unwrap_or(self.story.meta.text.color),
                size: self.story.meta.text.size * look.size.unwrap_or(1.0),
            },
            (Pending::Choice, Some(EventKind::Choice { prompt, .. })) => Prompt::Choice { prompt: plain_text(prompt, &st.vars), options: self.options().into_iter().map(|o| OptionView { id: o.id.clone(), text: plain_text(&o.text, &st.vars) }).collect() },
            (Pending::Delay(s) | Pending::Wait(s), _) => Prompt::Wait { seconds: *s },
            (Pending::Wardrobe(slots), _) => Prompt::Wardrobe { slots: slots.clone() },
            (Pending::Ended(title), _) => Prompt::Ended { title: title.clone() },
            (Pending::Error(message), _) => Prompt::Error { message: message.clone() },
            (p, _) => Prompt::Error { message: format!("Внутренняя ошибка движка: {p:?}") },
        };
        View {
            scene: st.scene.clone(),
            event: st.event.clone(),
            background: st.stage.background.clone(),
            bg_fade: st.stage.bg_fade,
            black: st.stage.black,
            fade: st.stage.fade,
            hide_fade: st.stage.hide_fade,
            sprites: sprites(&self.story, &st.stage, &st.outfit),
            music: st.stage.music.clone(),
            music_fade: st.stage.music_fade,
            dialog: st.stage.dialog,
            prompt,
        }
    }

    /// Гардероб игрока: открытые предметы по слотам. Пустой `allowed` — все слоты.
    pub fn wardrobe(&self, allowed: &[SlotId]) -> Vec<SlotView> {
        let w = &self.story.wardrobe;
        w.slots
            .iter()
            .filter(|s| allowed.is_empty() || allowed.contains(&s.id))
            .map(|s| SlotView {
                id: s.id.clone(),
                name: s.name.clone(),
                items: w.items.iter().filter(|i| i.slot == s.id && self.state.unlocked.contains(&i.id)).map(|i| ItemView { id: i.id.clone(), name: i.name.clone(), image: i.image.clone(), equipped: self.state.outfit.get(&s.id) == Some(&i.id) }).collect(),
            })
            .collect()
    }

    /// Слои героя в текущей одежде — для окна гардероба.
    pub fn hero_layers(&self) -> Vec<String> {
        let Some(hero) = self.story.wardrobe.hero.as_ref().and_then(|id| self.story.character(id)) else { return vec![] };
        let expression = self.state.stage.characters.iter().find(|c| c.character == hero.id).map(|c| c.expression.as_str()).unwrap_or("");
        sprite_layers(&self.story, &self.state.outfit, hero, expression)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::tests::{scene, story};

    #[test]
    fn text_substitutes_variables_and_marks_emphasis() {
        let vars: Vars = [("player_name".to_string(), Value::Text("Аня".into())), ("money".to_string(), Value::Int(5))].into_iter().collect();
        assert_eq!(substitute("Привет, {player_name}! У тебя {money}, {unknown}.", &vars), "Привет, Аня! У тебя 5, {unknown}.");
        assert_eq!(spans("Это *важно*, {player_name}", &vars), vec![Span { text: "Это ".into(), bold: false }, Span { text: "важно".into(), bold: true }, Span { text: ", Аня".into(), bold: false }]);
        assert_eq!(substitute("скобка { без пары", &vars), "скобка { без пары");
    }

    fn hero_story() -> Story {
        let hero = Character { id: "hero".into(), name: "Герой".into(), description: String::new(), color: [1, 2, 3], expressions: vec![Expression { name: "base".into(), image: "body.png".into() }], height: 0.8 };
        let mut s = story(vec![scene(
            "a",
            vec![
                Event::new(EventKind::ShowCharacter { character: "hero".into(), expression: "base".into(), pos: Pos { x: 0.5, y: 1.0, scale: 0.5 }, fade: 0.0 }),
                Event::new(EventKind::SetOutfit { slot: "top".into(), item: Some("shirt".into()) }),
                Event::new(EventKind::SetOutfit { slot: "hair_back".into(), item: Some("tail".into()) }),
                Event::new(EventKind::Say { speaker: Some("hero".into()), style: TextStyle::Dialogue, text: "Я *готов*".into(), expression: None, speed: None, sound: None, look: TextLook::default() }),
            ],
        )]);
        s.characters = vec![hero];
        s.wardrobe = Wardrobe {
            hero: Some("hero".into()),
            slots: vec![Slot { id: "hair_back".into(), name: "Волосы сзади".into(), behind: true }, Slot { id: "top".into(), name: "Верх".into(), behind: false }],
            items: vec![Item { id: "shirt".into(), slot: "top".into(), name: "Рубашка".into(), image: "shirt.png".into(), unlocked: true }, Item { id: "tail".into(), slot: "hair_back".into(), name: "Хвост".into(), image: "tail.png".into(), unlocked: false }],
            anytime: true,
        };
        s
    }

    #[test]
    fn line_look_overrides_project_text_settings_only_where_set() {
        let mut s = hero_story();
        s.meta.text = TextSettings { size: 0.04, color: [9, 9, 9], speed: 40.0 };
        let look = |s: &Story| match Runtime::new(s.clone()).view().prompt {
            Prompt::Text { color, size, .. } => (color, size),
            other => panic!("{other:?}"),
        };
        assert_eq!(look(&s), ([9, 9, 9], 0.04));
        if let EventKind::Say { look, .. } = &mut s.scenes[0].events[3].kind {
            *look = TextLook { color: Some([255, 0, 0]), size: Some(1.5) };
        }
        let (color, size) = look(&s);
        assert_eq!(color, [255, 0, 0]);
        assert!((size - 0.06).abs() < 1e-6);
    }

    #[test]
    fn old_project_files_without_look_still_load_and_stay_unchanged() {
        let json = r#"{"id":"e","type":"say","speaker":null,"text":"x"}"#;
        let e: Event = serde_json::from_str(json).unwrap();
        assert!(matches!(&e.kind, EventKind::Say { look, .. } if look.is_default()));
        assert!(!serde_json::to_string(&e).unwrap().contains("look"), "обычная реплика не обрастает новым полем");
    }

    #[test]
    fn hero_sprite_is_layered_behind_body_front() {
        let v = Runtime::new(hero_story()).view();
        assert_eq!(v.sprites[0].layers, vec!["tail.png", "body.png", "shirt.png"]);
        assert!((v.sprites[0].height - 0.4).abs() < 1e-6);
        match v.prompt {
            Prompt::Text { speaker: Some(sp), spans, seen, .. } => {
                assert_eq!((sp.name.as_str(), sp.color), ("Герой", [1, 2, 3]));
                assert_eq!(spans.len(), 2);
                assert!(!seen);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn wardrobe_lists_only_unlocked_items_and_marks_equipped() {
        let rt = Runtime::new(hero_story());
        let w = rt.wardrobe(&[]);
        assert_eq!(w.len(), 2);
        assert!(w[0].items.is_empty(), "хвост не открыт, хотя надет сюжетом");
        assert_eq!(w[1].items, vec![ItemView { id: "shirt".into(), name: "Рубашка".into(), image: "shirt.png".into(), equipped: true }]);
        assert_eq!(rt.wardrobe(&["top".into()]).len(), 1);
        assert_eq!(rt.hero_layers(), vec!["tail.png", "body.png", "shirt.png"]);
    }
}
