//! Правила проверки проекта и редкие ветки движка — по одному случаю на правило.
//! Написано по итогам мутационного тестирования: эти места можно было сломать незаметно.
use std::collections::BTreeSet;
use vn_core::cond::Cond;
use vn_core::model::*;
use vn_core::runtime::{Pending, Runtime};
use vn_core::save::{Restored, SaveError};
use vn_core::templates::{duplicate_chapter, duplicate_scene, Template};
use vn_core::validate::{validate, Severity};

fn ev(id: &str, kind: EventKind) -> Event {
    Event { id: id.into(), delay: 0.0, kind }
}

fn say(id: &str, speaker: Option<&str>, text: &str) -> Event {
    ev(id, EventKind::Say { speaker: speaker.map(Into::into), style: TextStyle::Dialogue, text: text.into(), expression: speaker.map(|_| "smile".into()), speed: None, sound: Some("assets/sounds/v.ogg".into()), look: TextLook::default() })
}

fn files() -> BTreeSet<String> {
    ["assets/backgrounds/a.png", "assets/characters/ann.png", "assets/clothing/coat.png", "assets/music/m.ogg", "assets/sounds/s.ogg", "assets/sounds/v.ogg"].into_iter().map(String::from).collect()
}

/// История, в которой есть событие каждого вида и всё в порядке.
fn rich() -> Story {
    let mut s = Story::new("Полная");
    s.meta.id = "story".into();
    s.chapters = vec![Chapter { id: "c1".into(), name: "Глава".into() }];
    s.variables = vec![VarDef { name: "trust".into(), initial: Value::Int(0), description: String::new() }];
    s.characters = vec![Character { id: "ann".into(), name: "Анна".into(), description: String::new(), color: [1, 2, 3], expressions: vec![Expression { name: "smile".into(), image: "assets/characters/ann.png".into() }], height: 0.9 }];
    s.wardrobe = Wardrobe { hero: Some("ann".into()), slots: vec![Slot { id: "top".into(), name: "Верх".into(), behind: false }], items: vec![Item { id: "coat".into(), slot: "top".into(), name: "Пальто".into(), image: "assets/clothing/coat.png".into(), unlocked: false }], anytime: false };
    let mut option = ChoiceOption::new("Довериться {trust}", Some("b".into()));
    option.id = "o1".into();
    option.cond = Some(Cond::parse("trust >= 0").unwrap());
    option.effects = vec![VarOp { var: "trust".into(), action: VarAction::Add, value: Value::Int(1) }];
    let mut leave = ChoiceOption::new("Уйти", Some("c".into()));
    leave.id = "o2".into();
    let a = vec![
        ev("a1", EventKind::Background { image: "assets/backgrounds/a.png".into(), fade: 0.0 }),
        ev("a2", EventKind::PlayMusic { track: "assets/music/m.ogg".into(), volume: 1.0, fade: 0.0, looped: true }),
        ev("a3", EventKind::ShowCharacter { character: "ann".into(), expression: "smile".into(), pos: Pos::CENTER, fade: 0.0 }),
        ev("a4", EventKind::MoveCharacter { character: "ann".into(), pos: Pos::LEFT, duration: 0.0 }),
        ev("a5", EventKind::Expression { character: "ann".into(), expression: "smile".into() }),
        ev("a6", EventKind::UnlockItem { item: "coat".into() }),
        ev("a7", EventKind::SetOutfit { slot: "top".into(), item: Some("coat".into()) }),
        ev("a8", EventKind::OpenWardrobe { slots: vec!["top".into()] }),
        ev("a9", EventKind::Sound { file: "assets/sounds/s.ogg".into(), volume: 1.0 }),
        ev("a10", EventKind::SetVar(VarOp { var: "trust".into(), action: VarAction::Add, value: Value::Int(1) })),
        say("a11", Some("ann"), "Доверие: {trust}"),
        ev("a12", EventKind::HideCharacter { character: "ann".into(), fade: 0.0 }),
        ev("a13", EventKind::Branch { cond: Cond::parse("trust > 5").unwrap(), scene: "c".into() }),
        ev("a14", EventKind::Choice { prompt: String::new(), options: vec![option, leave] }),
    ];
    s.scenes = vec![
        Scene { id: "a".into(), name: "А".into(), description: String::new(), chapter: Some("c1".into()), events: a },
        Scene { id: "b".into(), name: "Б".into(), description: String::new(), chapter: Some("c1".into()), events: vec![say("b1", None, "Вместе."), ev("b2", EventKind::End { title: "Хорошая".into() })] },
        Scene { id: "c".into(), name: "В".into(), description: String::new(), chapter: None, events: vec![say("c1", None, "Одна."), ev("c2", EventKind::Goto { scene: "b".into() })] },
    ];
    s.meta.start_scene = Some("a".into());
    s
}

fn kind_mut<'a>(s: &'a mut Story, scene: &str, event: &str) -> &'a mut EventKind {
    let sc = s.scene_mut(&scene.into()).unwrap();
    let i = sc.event_index(&event.into()).unwrap();
    &mut sc.events[i].kind
}

fn messages(s: &Story, severity: Severity) -> Vec<String> {
    validate(s, &files()).into_iter().filter(|i| i.severity == severity).map(|i| i.message).collect()
}

#[test]
fn story_using_every_event_kind_correctly_is_clean() {
    let issues = validate(&rich(), &files());
    assert!(issues.is_empty(), "{issues:#?}");
}

type Break = fn(&mut Story);

#[test]
fn every_breakage_is_reported_as_an_error_naming_the_cause() {
    let cases: Vec<(&str, Break)> = vec![
        ("Персонаж «bob» не существует", |s| *kind_mut(s, "a", "a3") = EventKind::ShowCharacter { character: "bob".into(), expression: String::new(), pos: Pos::CENTER, fade: 0.0 }),
        ("Персонаж «bob» не существует", |s| *kind_mut(s, "a", "a12") = EventKind::HideCharacter { character: "bob".into(), fade: 0.0 }),
        ("Персонаж «bob» не существует", |s| *kind_mut(s, "a", "a4") = EventKind::MoveCharacter { character: "bob".into(), pos: Pos::LEFT, duration: 0.0 }),
        ("Слот одежды «hat» не существует", |s| *kind_mut(s, "a", "a7") = EventKind::SetOutfit { slot: "hat".into(), item: None }),
        ("Предмет одежды «scarf» не существует", |s| *kind_mut(s, "a", "a7") = EventKind::SetOutfit { slot: "top".into(), item: Some("scarf".into()) }),
        ("Предмет «Пальто» не подходит к слоту «legs»", |s| {
            s.wardrobe.slots.push(Slot { id: "legs".into(), name: "Низ".into(), behind: false });
            *kind_mut(s, "a", "a7") = EventKind::SetOutfit { slot: "legs".into(), item: Some("coat".into()) };
        }),
        ("Предмет одежды «scarf» не существует", |s| *kind_mut(s, "a", "a6") = EventKind::UnlockItem { item: "scarf".into() }),
        ("главный герой не назначен", |s| s.wardrobe.hero = None),
        ("Слот одежды «hat» не существует", |s| *kind_mut(s, "a", "a8") = EventKind::OpenWardrobe { slots: vec!["hat".into()] }),
        ("Звук: файл «assets/sounds/none.ogg» отсутствует", |s| *kind_mut(s, "a", "a9") = EventKind::Sound { file: "assets/sounds/none.ogg".into(), volume: 1.0 }),
        ("Звук реплики: файл «x.ogg» отсутствует", |s| {
            if let EventKind::Say { sound, .. } = kind_mut(s, "a", "a11") {
                *sound = Some("x.ogg".into());
            }
        }),
        ("Музыка: файл не выбран", |s| *kind_mut(s, "a", "a2") = EventKind::PlayMusic { track: String::new(), volume: 1.0, fade: 0.0, looped: true }),
        ("Переход ведёт в несуществующую сцену «zzz»", |s| *kind_mut(s, "c", "c2") = EventKind::Goto { scene: "zzz".into() }),
        ("Условный переход ведёт в несуществующую сцену «zzz»", |s| *kind_mut(s, "a", "a13") = EventKind::Branch { cond: Cond::parse("trust > 5").unwrap(), scene: "zzz".into() }),
        ("Выбор без вариантов", |s| *kind_mut(s, "a", "a14") = EventKind::Choice { prompt: String::new(), options: vec![] }),
        ("Сцена «В» пуста", |s| s.scene_mut(&"c".into()).unwrap().events.clear()),
        ("Сцена «Б» обрывается", |s| drop(s.scene_mut(&"b".into()).unwrap().events.pop())),
        ("Сцена «А» обрывается", |s| {
            // Все варианты под условием: игрок может остаться без единого варианта.
            if let EventKind::Choice { options, .. } = kind_mut(s, "a", "a14") {
                options[1].cond = Some(Cond::parse("trust > 100").unwrap());
            }
        }),
        ("Не выбрана начальная сцена", |s| s.meta.start_scene = None),
        ("Начало истории ведёт в несуществующую сцену", |s| s.meta.start_scene = Some("zzz".into())),
        ("Запасная сцена для старых сохранений ведёт в несуществующую сцену", |s| s.meta.fallback_scene = Some("zzz".into())),
        ("ID «b1» используется 2 раза", |s| s.scene_mut(&"c".into()).unwrap().events[0].id = "b1".into()),
        ("без ID", |s| s.scene_mut(&"c".into()).unwrap().events[0].id = "".into()),
        ("Главный герой гардероба не существует", |s| s.wardrobe.hero = Some("bob".into())),
        ("Одежда «Пальто» привязана к несуществующему слоту", |s| s.wardrobe.items[0].slot = "hat".into()),
        ("Одежда «Пальто»: файл «nope.png» отсутствует", |s| s.wardrobe.items[0].image = "nope.png".into()),
        ("Персонаж «Анна», выражение «smile»: файл не выбран", |s| s.characters[0].expressions[0].image.clear()),
    ];
    for (expected, breakage) in cases {
        let mut s = rich();
        breakage(&mut s);
        let errors = messages(&s, Severity::Error);
        assert!(errors.iter().any(|m| m.contains(expected)), "ожидалась ошибка «{expected}», получено: {errors:?}");
    }
}

#[test]
fn doubtful_places_are_warnings_that_do_not_block_the_build() {
    let cases: Vec<(&str, Break)> = vec![
        ("нет выражения «angry»", |s| *kind_mut(s, "a", "a5") = EventKind::Expression { character: "ann".into(), expression: "angry".into() }),
        ("Переменная «luck» не определена", |s| *kind_mut(s, "a", "a10") = EventKind::SetVar(VarOp { var: "luck".into(), action: VarAction::Set, value: Value::Int(1) })),
        ("Переменная «luck» не определена", |s| *kind_mut(s, "a", "a13") = EventKind::Branch { cond: Cond::parse("luck > 1").unwrap(), scene: "c".into() }),
        ("Переменная «b» не определена", |s| *kind_mut(s, "b", "b1") = say("b1", None, "{trust} и {b}").kind),
        ("Сцена «А» относится к удалённой главе", |s| s.chapters.clear()),
        ("События после этого никогда не выполнятся", |s| s.scene_mut(&"b".into()).unwrap().events.insert(0, ev("b0", EventKind::Goto { scene: "c".into() }))),
        ("Перенаправление сохранений из удалённой сцены «old»", |s| drop(s.redirects.insert("old".into(), "gone".into()))),
        ("Вариант выбора без текста", |s| {
            if let EventKind::Choice { options, .. } = kind_mut(s, "a", "a14") {
                options[1].text = "  ".into();
            }
        }),
    ];
    for (expected, breakage) in cases {
        let mut s = rich();
        breakage(&mut s);
        let (warnings, errors) = (messages(&s, Severity::Warning), messages(&s, Severity::Error));
        assert!(warnings.iter().any(|m| m.contains(expected)), "ожидалось предупреждение «{expected}», получено: {warnings:?}");
        assert!(errors.is_empty(), "«{expected}» не должно блокировать сборку: {errors:?}");
    }
    let mut chained = rich();
    chained.redirects.insert("old".into(), "older".into());
    chained.redirects.insert("older".into(), "b".into());
    assert!(validate(&chained, &files()).is_empty(), "цепочка перенаправлений, приводящая к живой сцене, — не замечание");
}

fn play_to_choice(story: Story) -> Runtime {
    let mut rt = Runtime::new(story);
    for _ in 0..20 {
        if matches!(rt.pending(), Pending::Choice) {
            break;
        }
        rt.advance();
    }
    rt
}

#[test]
fn branch_jumps_only_when_its_condition_holds() {
    let rt = play_to_choice(rich());
    assert_eq!((rt.state.scene.as_str(), rt.state.vars["trust"].clone()), ("a", Value::Int(1)), "условие ложно — сцена продолжается до выбора");
    let mut s = rich();
    *kind_mut(&mut s, "a", "a13") = EventKind::Branch { cond: Cond::parse("trust >= 1").unwrap(), scene: "c".into() };
    let mut rt = Runtime::new(s);
    for _ in 0..20 {
        if rt.state.scene.as_str() == "c" {
            break;
        }
        rt.advance();
    }
    assert_eq!(rt.state.scene.as_str(), "c", "условие истинно — переход в сцену В");
    assert_eq!(rt.state.backlog.last().unwrap().text, "Одна.");
}

#[test]
fn variables_never_set_before_count_from_zero() {
    let mut vars = Vars::new();
    VarOp { var: "debt".into(), action: VarAction::Sub, value: Value::Int(7) }.apply(&mut vars);
    VarOp { var: "luck".into(), action: VarAction::Add, value: Value::Int(3) }.apply(&mut vars);
    assert_eq!((vars["debt"].clone(), vars["luck"].clone()), (Value::Int(-7), Value::Int(3)));
}

#[test]
fn conditions_order_only_what_can_be_ordered_and_print_back_readably() {
    let vars: Vars = [("flag".to_string(), Value::Bool(true)), ("n".to_string(), Value::Int(-3)), ("name".to_string(), Value::Text("b".into()))].into_iter().collect();
    let check = |t: &str| Cond::parse(t).unwrap().eval(&vars);
    assert!(!check("flag > нет") && !check("flag >= да") && check("flag == да"), "да/нет сравниваются только на равенство");
    assert!(check("n == -3") && check("n < -2") && check("n>-4"), "отрицательные числа и запись без пробелов");
    assert!(check("name > \"a\"") && check("name < \"c\""));
    assert!(Cond::parse("n - 3").is_err(), "минус — не операция");
    for (text, printed) in [("name == \"два слова\"", "name == \"два слова\""), ("name == b", "name == b"), ("name == \"5\"", "name == \"5\""), ("name == \"да\"", "name == \"да\""), ("name == \"\"", "name == \"\"")] {
        let c = Cond::parse(text).unwrap();
        assert_eq!(c.to_string(), printed);
        assert_eq!(Cond::parse(&c.to_string()).unwrap(), c);
    }
    for (text, brackets) in [("не (a или b) и c", true), ("не a и (не b)", false), ("a и b или c", false), ("a и (b или c)", true)] {
        let c = Cond::parse(text).unwrap();
        assert_eq!(c.to_string().contains('('), brackets, "скобки только там, где они меняют смысл: {text} → {c}");
        assert_eq!(Cond::parse(&c.to_string()).unwrap(), c, "{text} → {c}");
    }
}

#[test]
fn story_lists_its_files_and_all_three_kinds_of_links() {
    let s = rich();
    assert_eq!(s.assets_used(), files());
    let edges: Vec<(String, String, String)> = s.edges().into_iter().map(|e| (e.from.0, e.label, e.to.0)).collect();
    assert_eq!(edges, vec![("a".into(), "если trust > 5".into(), "c".into()), ("a".into(), "Довериться {trust}".into(), "b".into()), ("a".into(), "Уйти".into(), "c".into()), ("c".into(), String::new(), "b".into())]);
    assert!(EventKind::End { title: String::new() }.title() == "Концовка" && Template::ALL.iter().all(|t| t.name().chars().count() > 3));
}

#[test]
fn removing_a_scene_repoints_older_redirects_to_its_replacement() {
    let mut s = rich();
    s.redirects.insert("ancient".into(), "c".into());
    s.remove_scene(&"c".into(), Some("b".into()));
    assert_eq!(s.redirects.get(&SceneId::from("ancient")), Some(&SceneId::from("b")), "старое перенаправление не повисло на удалённой сцене");
    assert_eq!(s.redirects.get(&SceneId::from("c")), Some(&SceneId::from("b")));
}

#[test]
fn save_from_a_twice_removed_scene_follows_the_chain_and_broken_chains_fall_back() {
    let story = rich();
    let save = Runtime::start_at(story.clone(), "c".into()).save(0);
    assert_eq!(save.state.scene.as_str(), "c");
    let mut updated = story.clone();
    updated.scenes.retain(|s| s.id.as_str() != "c");
    *kind_mut(&mut updated, "a", "a13") = say("a13", None, "…").kind;
    if let EventKind::Choice { options, .. } = kind_mut(&mut updated, "a", "a14") {
        options[1].goto = Some("b".into());
    }
    updated.redirects.insert("c".into(), "c2".into());
    updated.redirects.insert("c2".into(), "b".into());
    let (rt2, how) = Runtime::restore(updated.clone(), save.clone()).unwrap();
    assert_eq!(rt2.state.scene.as_str(), "b");
    assert_eq!(how, Restored::Redirected("b".into()));
    updated.redirects.insert("c2".into(), "nowhere".into());
    let Err(e) = Runtime::restore(updated.clone(), save.clone()) else { panic!("цепочка в никуда и нет запасной сцены") };
    assert_eq!(e, SaveError::Incompatible);
    assert!(e.to_string().chars().count() > 20, "игрок получает объяснение, а не пустую строку: «{e}»");
    updated.meta.fallback_scene = Some("a".into());
    assert_eq!(Runtime::restore(updated, save).unwrap().0.state.scene.as_str(), "a");
}

#[test]
fn copied_chapter_links_its_copies_to_each_other_and_keeps_outside_links() {
    let mut s = rich();
    s.scene_mut(&"c".into()).unwrap().chapter = None;
    s.scenes.push(Scene { id: "d".into(), name: "Г".into(), description: String::new(), chapter: Some("c1".into()), events: vec![ev("d1", EventKind::Branch { cond: Cond::parse("trust > 0").unwrap(), scene: "b".into() }), ev("d2", EventKind::Goto { scene: "a".into() })] });
    let new = duplicate_chapter(&mut s, &"c1".into()).unwrap();
    let copies: Vec<&Scene> = s.scenes.iter().filter(|x| x.chapter.as_ref() == Some(&new)).collect();
    assert_eq!(copies.len(), 3);
    let id_of = |name_start: &str| copies.iter().find(|c| c.name.starts_with(name_start)).unwrap().id.clone();
    let (a2, b2, d2) = (id_of("А"), id_of("Б"), id_of("Г"));
    let targets: Vec<(SceneId, SceneId)> = s.edges().into_iter().filter(|e| [&a2, &b2, &d2].contains(&&e.from)).map(|e| (e.from, e.to)).collect();
    assert!(targets.contains(&(a2.clone(), b2.clone())), "выбор в копии ведёт в копию: {targets:?}");
    assert!(targets.contains(&(a2.clone(), "c".into())), "ссылка наружу главы осталась на оригинал");
    assert!(targets.contains(&(d2.clone(), b2)) && targets.contains(&(d2, a2)), "условие и переход в копии ведут в копии");
    assert!(validate(&s, &files()).iter().all(|i| i.severity != Severity::Error), "копия не ломает проект (ID не повторяются)");
    let single = duplicate_scene(&mut s, &"b".into()).unwrap();
    assert!(!s.layout.contains_key(&single) || s.layout.get(&single) != s.layout.get(&SceneId::from("b")), "копия не ложится точно на оригинал");
}
