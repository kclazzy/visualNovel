//! Игра глазами игрока: щелчки по кнопкам и клавиши, проверяется то, что он увидит.
use egui::Key;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::sync::Arc;
use vn_core::model::*;
use vn_core::runtime::Pending;
use vn_store::saves::{SaveStore, QUICK};
use vn_view::{DirAssets, GameView};

fn say(text: &str) -> Event {
    Event::new(EventKind::Say { speaker: None, style: TextStyle::Narration, text: text.into(), expression: None, speed: None, sound: None })
}

fn story() -> Story {
    let mut s = Story::new("Проба");
    let mut good = Scene::new("Хорошо");
    good.events = vec![say("Спасибо"), Event::new(EventKind::Wait { seconds: 0.5 }), say("После паузы"), Event::new(EventKind::End { title: "Дружба".into() })];
    let mut help = ChoiceOption::new("Помочь", Some(good.id.clone()));
    help.effects = vec![VarOp { var: "trust".into(), action: VarAction::Add, value: Value::Int(1) }];
    s.scenes[0].events = vec![say("Первая"), say("Вторая"), Event::new(EventKind::Choice { prompt: "Что делать?".into(), options: vec![help, ChoiceOption::new("Уйти", None)] }), Event::new(EventKind::End { title: "Одиночество".into() })];
    s.scenes.push(good);
    s.variables = vec![VarDef { name: "trust".into(), initial: Value::Int(0), description: String::new() }];
    s
}

fn harness(dir: &std::path::Path) -> Harness<'static, GameView> {
    harness_for(story(), dir)
}

fn harness_for(story: Story, dir: &std::path::Path) -> Harness<'static, GameView> {
    let game = GameView::new(story, Arc::new(DirAssets(dir.to_path_buf())), SaveStore::at(dir.join("saves")));
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 720.0)).with_max_steps(400).with_context_setup(vn_view::install_fonts).build_ui_state(|ui, game: &mut GameView| game.ui(ui), game);
    h.run();
    h
}

fn last_line(h: &Harness<'_, GameView>) -> String {
    h.state().runtime().and_then(|rt| rt.state.backlog.last()).map(|l| l.text.clone()).unwrap_or_default()
}

fn press(h: &mut Harness<'_, GameView>, key: Key) {
    h.key_press(key);
    h.run();
}

#[test]
fn player_reads_chooses_and_reaches_an_ending() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(tmp.path());
    assert!(h.state().runtime().is_none(), "сначала главное меню");
    h.get_by_label("Новая игра").click();
    h.run();
    assert_eq!(last_line(&h), "Первая");
    press(&mut h, Key::Space);
    assert_eq!(last_line(&h), "Вторая");
    press(&mut h, Key::Enter);
    assert_eq!(*h.state().runtime().unwrap().pending(), Pending::Choice);
    press(&mut h, Key::Space);
    assert_eq!(*h.state().runtime().unwrap().pending(), Pending::Choice, "пробел не проскакивает выбор");
    h.get_by_label("1. Помочь").click();
    h.run();
    assert_eq!(last_line(&h), "Спасибо");
    assert_eq!(h.state().runtime().unwrap().state.vars["trust"], Value::Int(1));
    press(&mut h, Key::Space);
    assert_eq!(last_line(&h), "После паузы", "пауза 0,5 с проходит сама");
    press(&mut h, Key::Space);
    assert_eq!(*h.state().runtime().unwrap().pending(), Pending::Ended("Дружба".into()));
    h.get_by_label("Дружба");
    h.get_by_label("В главное меню").click();
    h.run();
    assert!(h.state().runtime().is_none());
}

#[test]
fn quick_save_and_load_return_to_the_same_line_and_continue_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let the_story = story();
    let mut h = harness_for(the_story.clone(), tmp.path());
    h.get_by_label("Новая игра").click();
    h.run();
    press(&mut h, Key::Space);
    press(&mut h, Key::F5);
    assert!(SaveStore::at(tmp.path().join("saves")).read(QUICK).is_some());
    press(&mut h, Key::Space);
    h.get_by_label("2. Уйти").click();
    h.run();
    assert_eq!(*h.state().runtime().unwrap().pending(), Pending::Ended("Одиночество".into()));
    h.get_by_label("В главное меню").click();
    h.run();
    drop(h);

    // Игра закрыта и запущена снова: «Продолжить» берёт самое свежее сохранение (автосохранение у выбора).
    let mut h = harness_for(the_story, tmp.path());
    h.get_by_label("Продолжить").click();
    h.run();
    assert_eq!(*h.state().runtime().unwrap().pending(), Pending::Ended("Одиночество".into()));
    h.get_by_label("В главное меню").click();
    h.run();
    h.get_by_label("Загрузить").click();
    h.run();
    h.get_by_label_contains("Быстрое сохранение").click();
    h.run();
    assert_eq!(last_line(&h), "Вторая");
    assert_eq!(h.state().runtime().unwrap().state.choices.len(), 0, "выбор ещё не сделан");
}

#[test]
fn menu_saves_into_a_slot_and_settings_persist() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(tmp.path());
    h.get_by_label("Новая игра").click();
    h.run();
    press(&mut h, Key::Escape);
    h.get_by_label("Сохранить").click();
    h.run();
    h.get_by_label_contains("Слот 3").click();
    h.run();
    let store = SaveStore::at(tmp.path().join("saves"));
    assert!(store.read("slot3").unwrap().unwrap().label.contains("Первая"));
    h.get_by_label("← Назад").click();
    h.run();
    h.get_by_label("Настройки").click();
    h.run();
    h.state_mut().settings.music = 0.25;
    h.get_by_label("Полный экран (F11)").click();
    h.run();
    assert!(store.settings().fullscreen);
    assert_eq!(store.settings().music, 0.25);
    press(&mut h, Key::Escape);
    h.get_by_label("Вернуться в игру").click();
    h.run();
    press(&mut h, Key::Space);
    assert_eq!(last_line(&h), "Вторая");
}

#[test]
fn empty_slot_and_missing_save_do_not_break_the_game() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(tmp.path());
    h.get_by_label("Новая игра").click();
    h.run();
    press(&mut h, Key::F9);
    assert_eq!(last_line(&h), "Первая", "быстрой загрузки нет — игра продолжается");
}
