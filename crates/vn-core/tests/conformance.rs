//! Эталонные трассы движка для веб-плеера.
//! Веб-плеер (player-web/runtime.js) повторяет рантайм на JavaScript; этот тест
//! записывает, что именно показывает движок на каждом шаге, а `node --test` проверяет,
//! что JS-версия показывает то же самое. Обновить эталон: `UPDATE_TRACES=1 cargo test -p vn-core`.
use serde_json::{json, Value as Json};
use vn_core::cond::Cond;
use vn_core::model::*;
use vn_core::runtime::Runtime;
use vn_core::save::{parse_save, Restored, SaveError};

fn ev(id: &str, kind: EventKind) -> Event {
    Event { id: id.into(), delay: 0.0, kind }
}

fn say(id: &str, speaker: Option<&str>, style: TextStyle, text: &str) -> Event {
    ev(id, EventKind::Say { speaker: speaker.map(Into::into), style, text: text.into(), expression: None, speed: None, sound: None, look: TextLook::default() })
}

fn scene(id: &str, events: Vec<Event>) -> Scene {
    Scene { id: id.into(), name: format!("Сцена {id}"), description: String::new(), chapter: None, events }
}

fn story() -> Story {
    let mut s = Story::new("Эталон");
    s.meta.id = "story_fixture".into();
    s.meta.start_scene = Some("start".into());
    s.characters = vec![
        Character { id: "ann".into(), name: "Анна".into(), description: String::new(), color: [255, 170, 190], expressions: vec![Expression { name: "smile".into(), image: "assets/characters/ann_smile.png".into() }, Expression { name: "sad".into(), image: "assets/characters/ann_sad.png".into() }], height: 0.9 },
        Character { id: "hero".into(), name: "Герой".into(), description: String::new(), color: [140, 200, 255], expressions: vec![Expression { name: "base".into(), image: "assets/characters/hero.png".into() }], height: 0.85 },
        Character { id: "ghost".into(), name: "Тень".into(), description: String::new(), color: [9, 9, 9], expressions: vec![], height: 0.9 },
    ];
    s.variables = vec![
        VarDef { name: "trust".into(), initial: Value::Int(0), description: String::new() },
        VarDef { name: "key".into(), initial: Value::Bool(false), description: String::new() },
        VarDef { name: "name".into(), initial: Value::Text("Саша".into()), description: String::new() },
    ];
    s.wardrobe = Wardrobe {
        hero: Some("hero".into()),
        slots: vec![Slot { id: "hair_back".into(), name: "Волосы сзади".into(), behind: true }, Slot { id: "top".into(), name: "Верх".into(), behind: false }, Slot { id: "hat".into(), name: "Шляпа".into(), behind: false }],
        items: vec![
            Item { id: "tail".into(), slot: "hair_back".into(), name: "Хвост".into(), image: "assets/clothing/tail.png".into(), unlocked: true },
            Item { id: "shirt".into(), slot: "top".into(), name: "Рубашка".into(), image: "assets/clothing/shirt.png".into(), unlocked: true },
            Item { id: "coat".into(), slot: "top".into(), name: "Пальто".into(), image: "assets/clothing/coat.png".into(), unlocked: false },
            Item { id: "cap".into(), slot: "hat".into(), name: "Кепка".into(), image: "assets/clothing/cap.png".into(), unlocked: false },
        ],
        anytime: true,
    };
    let mut delayed = say("e06", None, TextStyle::Narration, "Прошла секунда. Ключ: {key}, доверие: {trust}, {unknown}.");
    delayed.delay = 0.5;
    let options = vec![
        ChoiceOption { id: "o_help".into(), text: "Помочь, {name}".into(), cond: None, effects: vec![VarOp { var: "trust".into(), action: VarAction::Add, value: Value::Int(2) }, VarOp { var: "key".into(), action: VarAction::Set, value: Value::Bool(true) }], goto: Some("good".into()) },
        ChoiceOption { id: "o_secret".into(), text: "Тайный ход".into(), cond: Some(Cond::parse("trust >= 5 или не (key == нет)").unwrap()), effects: vec![], goto: Some("final".into()) },
        ChoiceOption { id: "o_stay".into(), text: "Остаться".into(), cond: Some(Cond::parse("name == Саша и trust < 3").unwrap()), effects: vec![VarOp { var: "trust".into(), action: VarAction::Sub, value: Value::Int(1) }, VarOp { var: "fresh".into(), action: VarAction::Add, value: Value::Int(4) }, VarOp { var: "name".into(), action: VarAction::Add, value: Value::Int(4) }], goto: None },
    ];
    s.scenes = vec![
        scene(
            "start",
            vec![
                ev("e01", EventKind::Background { image: "assets/backgrounds/room.png".into(), fade: 0.5 }),
                ev("e02", EventKind::PlayMusic { track: "assets/music/calm.ogg".into(), volume: 0.8, fade: 1.0, looped: true }),
                ev("e03", EventKind::ShowCharacter { character: "ann".into(), expression: "smile".into(), pos: Pos::LEFT, fade: 0.3 }),
                ev("e04", EventKind::ShowCharacter { character: "hero".into(), expression: "base".into(), pos: Pos { x: 0.7, y: 0.95, scale: 1.1 }, fade: 0.0 }),
                ev("e04b", EventKind::ShowCharacter { character: "ghost".into(), expression: String::new(), pos: Pos::CENTER, fade: 0.0 }),
                ev("e04c", EventKind::ShowCharacter { character: "nobody".into(), expression: String::new(), pos: Pos::CENTER, fade: 0.0 }),
                say("e05", Some("ann"), TextStyle::Dialogue, "Привет, {name}! Это *очень важно*."),
                delayed,
                ev("e07", EventKind::SetVar(VarOp { var: "trust".into(), action: VarAction::Add, value: Value::Int(1) })),
                ev("e08", EventKind::MoveCharacter { character: "ann".into(), pos: Pos::CENTER, duration: 0.8 }),
                ev("e09", EventKind::Expression { character: "ann".into(), expression: "sad".into() }),
                ev("e10", EventKind::DialogBox { pos: DialogPos::Top }),
                ev("e11", EventKind::Sound { file: "assets/sounds/door.wav".into(), volume: 0.6 }),
                ev("e12", EventKind::MusicVolume { volume: 0.3, fade: 0.5 }),
                ev("e13", EventKind::Wait { seconds: 1.0 }),
                ev("e14", EventKind::OpenWardrobe { slots: vec!["top".into(), "hat".into()] }),
                ev("e15", EventKind::UnlockItem { item: "coat".into() }),
                ev("e16", EventKind::SetOutfit { slot: "hair_back".into(), item: Some("tail".into()) }),
                ev("e17", EventKind::Fade { out: true, duration: 0.4 }),
                ev("e18", EventKind::Background { image: String::new(), fade: 0.0 }),
                ev("e19", EventKind::Fade { out: false, duration: 0.4 }),
                ev("e20", EventKind::Say { speaker: Some("ann".into()), style: TextStyle::Thought, text: "Что же выбрать…".into(), expression: Some("smile".into()), speed: Some(12.5), sound: Some("assets/sounds/voice.ogg".into()), look: TextLook { color: Some([255, 120, 0]), size: Some(1.5) } }),
                ev("e21", EventKind::Choice { prompt: "Что делать, {name}?".into(), options }),
                say("e22", Some("hero"), TextStyle::Dialogue, "Я остаюсь. Имя теперь: {name}, новое: {fresh}."),
                ev("e23", EventKind::SetOutfit { slot: "hair_back".into(), item: None }),
                ev("e24", EventKind::Branch { cond: Cond::parse("trust >= 2").unwrap(), scene: "good".into() }),
                ev("e25", EventKind::HideCharacter { character: "ann".into(), fade: 0.25 }),
                ev("e26", EventKind::StopMusic { fade: 2.0 }),
                ev("e27", EventKind::Choice { prompt: "Никто не увидит".into(), options: vec![ChoiceOption { id: "o_never".into(), text: "Скрыто".into(), cond: Some(Cond::parse("missing").unwrap()), effects: vec![], goto: None }] }),
                say("e28", None, TextStyle::System, "Системное сообщение."),
                ev("e29", EventKind::End { title: "Плохая концовка".into() }),
            ],
        ),
        scene("good", vec![say("g1", Some("ann"), TextStyle::Dialogue, "Спасибо!"), ev("g2", EventKind::Goto { scene: "final".into() })]),
        scene("final", vec![say("f1", None, TextStyle::Narration, "Финал."), ev("f2", EventKind::End { title: "Хорошая концовка".into() })]),
        scene("fall", vec![say("x1", None, TextStyle::Narration, "Сцена без концовки.")]),
        scene("lost", vec![ev("l1", EventKind::Goto { scene: "nowhere".into() })]),
        scene("spin", vec![ev("s1", EventKind::Goto { scene: "spin".into() })]),
    ];
    s
}

fn snapshot(cmd: Json, rt: &mut Runtime, ok: bool) -> Json {
    json!({ "cmd": cmd, "ok": ok, "cues": rt.take_cues(), "view": rt.view(), "state": rt.state, "wardrobe": rt.wardrobe(&[]), "hero_layers": rt.hero_layers() })
}

/// Сыграть сценарий команд, записывая картину после каждой.
fn play(start: Option<&str>, script: &[&[&str]]) -> (Json, Runtime) {
    let mut rt = match start {
        Some(s) => Runtime::start_at(story(), s.into()),
        None => Runtime::new(story()),
    };
    let mut steps = vec![snapshot(json!(["start"]), &mut rt, true)];
    for cmd in script {
        let ok = match *cmd {
            ["advance"] => {
                rt.advance();
                true
            }
            ["choose", id] => rt.choose(&(*id).into()),
            ["equip", slot, item] => rt.equip(&(*slot).into(), Some(&(*item).into())),
            ["unequip", slot] => rt.equip(&(*slot).into(), None),
            other => panic!("неизвестная команда {other:?}"),
        };
        steps.push(snapshot(json!(cmd), &mut rt, ok));
    }
    (json!({ "start": start, "steps": steps }), rt)
}

fn restore_case(name: &str, story: &Story, save_json: &str) -> Json {
    let outcome = match Runtime::restore(story.clone(), parse_save(save_json).unwrap()) {
        Ok((mut rt, how)) => {
            let how = match how {
                Restored::Exact => json!({"how": "exact"}),
                Restored::SceneRestart => json!({"how": "scene_restart"}),
                Restored::Redirected(to) => json!({"how": "redirected", "to": to}),
            };
            json!({ "result": how, "snapshot": snapshot(json!(["restore"]), &mut rt, true) })
        }
        Err(e) => json!({ "error": match e { SaveError::WrongStory => "wrong_story", SaveError::Incompatible => "incompatible", SaveError::TooNew(_) => "too_new", SaveError::Corrupt(_) => "corrupt" } }),
    };
    json!({ "name": name, "story": story, "save": serde_json::from_str::<Json>(save_json).unwrap(), "expect": outcome })
}

fn traces() -> Json {
    let to_choice: &[&[&str]] = &[&["advance"], &["advance"], &["advance"], &["advance"], &["equip", "top", "shirt"], &["equip", "top", "coat"], &["equip", "hat", "shirt"], &["equip", "hat", "cap"], &["unequip", "top"], &["equip", "top", "shirt"], &["advance"], &["equip", "top", "coat"], &["advance"]];
    let stay = [to_choice, &[&["choose", "o_secret"], &["choose", "o_stay"], &["advance"], &["advance"], &["advance"]]].concat();
    let help = [to_choice, &[&["choose", "o_help"], &["advance"], &["advance"], &["advance"], &["choose", "o_help"]]].concat();
    let (bad, _) = play(None, &stay);
    let (good, _) = play(None, &help);
    let (fall, _) = play(Some("fall"), &[&["advance"], &["advance"]]);
    let (lost, _) = play(Some("lost"), &[&["advance"]]);
    let (spin, _) = play(Some("spin"), &[]);
    let (missing, _) = play(Some("no_such_scene"), &[]);

    let (_, at_choice) = play(None, to_choice);
    let (_, mut in_good) = play(None, &[to_choice, &[&["choose", "o_help"]]].concat());
    let (_, in_delay) = play(None, &[&["advance"]]);
    let save_choice = serde_json::to_string(&at_choice.save(1_794_404_701)).unwrap();
    let save_good = serde_json::to_string(&in_good.save(1_794_404_702)).unwrap();
    let save_delay = serde_json::to_string(&in_delay.save(1_794_404_703)).unwrap();
    in_good.advance();

    let base = story();
    let mut grown = base.clone();
    grown.meta.story_version = 2;
    grown.variables.push(VarDef { name: "chapter2".into(), initial: Value::Int(7), description: String::new() });
    grown.wardrobe.items.retain(|i| i.id.as_str() != "coat");
    grown.wardrobe.items.push(Item { id: "scarf".into(), slot: "hat".into(), name: "Шарф".into(), image: "assets/clothing/scarf.png".into(), unlocked: true });
    grown.scenes[1].events.insert(0, say("g0", None, TextStyle::Narration, "Новое вступление."));
    let mut rewritten = base.clone();
    rewritten.scenes[1].events[0].id = "g1_new".into();
    let mut redirected = base.clone();
    redirected.remove_scene(&"good".into(), Some("final".into()));
    let mut chained = redirected.clone();
    chained.remove_scene(&"final".into(), Some("fall".into()));
    let mut fallback = base.clone();
    fallback.remove_scene(&"good".into(), None);
    fallback.meta.fallback_scene = Some("fall".into());
    let mut gone = base.clone();
    gone.remove_scene(&"good".into(), None);
    let mut foreign = base.clone();
    foreign.meta.id = "another".into();

    json!({
        "story": base,
        "plays": [bad, good, fall, lost, spin, missing],
        "restores": [
            restore_case("выбор, та же история", &base, &save_choice),
            restore_case("пауза перед событием", &base, &save_delay),
            restore_case("реплика, та же история", &base, &save_good),
            restore_case("история выросла", &grown, &save_good),
            restore_case("выбор, история выросла (удалён надетый предмет)", &grown, &save_choice),
            restore_case("событие переписано", &rewritten, &save_good),
            restore_case("сцена удалена, есть перенаправление", &redirected, &save_good),
            restore_case("цепочка перенаправлений", &chained, &save_good),
            restore_case("сцена удалена, есть запасная", &fallback, &save_good),
            restore_case("сцена удалена бесследно", &gone, &save_good),
            restore_case("чужая история", &foreign, &save_good),
        ],
        "save_sample": at_choice.save(1_794_404_701),
    })
}

#[test]
fn traces_match_the_committed_reference() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../player-web/tests/trace.json");
    let fresh = serde_json::to_string(&traces()).unwrap();
    if std::env::var("UPDATE_TRACES").is_ok() {
        std::fs::write(path, &fresh).unwrap();
    }
    let committed = std::fs::read_to_string(path).expect("нет эталона: запустите UPDATE_TRACES=1 cargo test -p vn-core");
    assert!(committed == fresh, "движок изменил поведение: обновите эталон (UPDATE_TRACES=1) и проверьте веб-плеер (node --test player-web/tests)");
}

#[test]
fn the_reference_script_really_reaches_both_endings() {
    let t = traces();
    let last_prompt = |play: usize| t["plays"][play]["steps"].as_array().unwrap().last().unwrap()["view"]["prompt"].clone();
    assert_eq!(last_prompt(0), json!({"kind": "ended", "title": "Плохая концовка"}));
    assert_eq!(last_prompt(1), json!({"kind": "ended", "title": "Хорошая концовка"}));
    assert_eq!(last_prompt(2), json!({"kind": "ended", "title": ""}));
    assert_eq!(last_prompt(3)["kind"], "error");
    assert_eq!(last_prompt(4)["kind"], "error");
    assert_eq!(last_prompt(5)["kind"], "error");
    let expects: Vec<String> = t["restores"].as_array().unwrap().iter().map(|r| r["expect"]["result"]["how"].as_str().or(r["expect"]["error"].as_str()).unwrap().to_string()).collect();
    assert_eq!(expects, ["exact", "exact", "exact", "exact", "exact", "scene_restart", "redirected", "redirected", "redirected", "incompatible", "wrong_story"]);
}
