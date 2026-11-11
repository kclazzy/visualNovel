//! Редактор глазами автора: щелчки по кнопкам, проверяется то, что окажется в проекте и на диске.
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;
use std::path::Path;
use vn_core::model::*;
use vn_studio::ai_panel::Mode;
use vn_studio::app::{Confirm, NewKind, Tab};
use vn_studio::Studio;
use vn_store::project;

fn harness(tmp: &Path) -> Harness<'static, Studio> {
    let studio = Studio::new(tmp.join("config/studio.json"));
    let mut h = Harness::builder().with_size(egui::vec2(1440.0, 860.0)).with_max_steps(40).with_accessibility_check(false).with_context_setup(vn_view::install_fonts).build_ui_state(|ui, s: &mut Studio| s.ui(ui), studio);
    h.run();
    h
}

fn with_project(tmp: &Path, kind: NewKind) -> Harness<'static, Studio> {
    let mut h = harness(tmp);
    h.state_mut().create_project(&tmp.join("novel"), "Проба", kind);
    h.run();
    assert!(h.state().error.is_none(), "{:?}", h.state().error);
    h
}

fn click(h: &mut Harness<'_, Studio>, label: &str) {
    h.get_all_by_label(label).next().unwrap_or_else(|| panic!("нет элемента «{label}»")).click();
    h.run();
}

fn story<'a>(h: &'a Harness<'_, Studio>) -> &'a Story {
    &h.state().project.as_ref().expect("проект открыт").story
}

#[test]
fn welcome_screen_offers_to_create_a_project_three_ways() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = harness(tmp.path());
    click(&mut h, "  Создать новый проект  ");
    h.get_by_label_contains("Вручную");
    h.get_by_label_contains("С помощью ИИ");
    h.get_by_label_contains("Демо-проект —");
    h.state_mut().new_dialog.as_mut().unwrap().folder = tmp.path().display().to_string();
    h.run();
    click(&mut h, "Создать");
    assert_eq!(story(&h).meta.name, "Моя новелла");
    assert!(tmp.path().join("Моя_новелла/project.json").exists() || std::fs::read_dir(tmp.path()).unwrap().flatten().any(|d| d.path().join("project.json").exists()));
}

#[test]
fn author_adds_scene_and_event_undoes_and_saves() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = with_project(tmp.path(), NewKind::Manual);
    click(&mut h, "+ Сцена");
    assert_eq!(story(&h).scenes.len(), 2);
    click(&mut h, "+ Событие");
    click(&mut h, "Текст автора");
    let p = h.state().project.as_ref().unwrap();
    let scene = p.current_scene().unwrap();
    assert_eq!(scene.name, "Сцена 2");
    assert!(matches!(scene.events[0].kind, EventKind::Say { speaker: None, .. }));
    assert_eq!(p.event, Some(scene.events[0].id.clone()), "новое событие выбрано для правки");
    h.step();
    click(&mut h, "↶ Отменить");
    assert!(story(&h).scenes.iter().all(|s| s.events.is_empty()), "отмена убрала событие");
    click(&mut h, "↷ Повторить");
    assert_eq!(story(&h).scenes[1].events.len(), 1);
    assert!(h.state().project.as_ref().unwrap().dirty());
    click(&mut h, "Сохранить");
    assert!(!h.state().project.as_ref().unwrap().dirty());
    assert_eq!(&project::load(&tmp.path().join("novel")).unwrap().story, story(&h), "на диске то же, что в редакторе");
}

#[test]
fn check_finds_broken_link_and_blocks_the_build() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = with_project(tmp.path(), NewKind::Demo);
    click(&mut h, "✔ Проверить");
    h.get_by_label_contains("замечаний нет");
    let p = h.state_mut().project.as_mut().unwrap();
    let victim = p.story.scenes[1].id.clone();
    p.confirm = Some(Confirm::Scene { id: victim.clone(), redirect: None });
    h.run();
    click(&mut h, "Удалить");
    assert!(story(&h).scene(&victim).is_none());
    h.get_by_label_contains("Проверка: ошибок — ");
    click(&mut h, "Собрать");
    click(&mut h, "Версия для телефона (веб / PWA)");
    let p = h.state().project.as_ref().unwrap();
    assert!(p.built.is_none() && p.status.contains("Сборка остановлена"), "{}", p.status);
    click(&mut h, "↶ Отменить");
    assert!(story(&h).scene(&victim).is_some(), "удаление сцены отменяется");
}

#[test]
fn phone_build_and_story_update_are_produced_from_the_menu() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = with_project(tmp.path(), NewKind::Demo);
    click(&mut h, "Собрать");
    click(&mut h, "Версия для телефона (веб / PWA)");
    let built = h.state().project.as_ref().unwrap().built.as_ref().expect("сборка выполнена").path.clone();
    for file in ["index.html", "player.js", "runtime.js", "story.json", "manifest.webmanifest", "sw.js"] {
        assert!(built.join(file).is_file(), "нет {file}");
    }
    assert!(built.with_extension("zip").is_file() || std::fs::read_dir(built.parent().unwrap()).unwrap().flatten().any(|f| f.path().extension().is_some_and(|e| e == "zip")));
    h.get_by_label_contains("Проверить на телефоне");
    click(&mut h, "Закрыть");

    click(&mut h, "Собрать");
    click(&mut h, "Выпустить обновление истории (.vnpak)…");
    h.state_mut().project.as_mut().unwrap().release.as_mut().unwrap().0 = "1.1".into();
    h.run();
    click(&mut h, "Выпустить");
    let p = h.state().project.as_ref().unwrap();
    assert_eq!((p.story.meta.story_version, p.story.meta.version.as_str()), (2, "1.1"));
    let pack = vn_store::pack::Pack::open(std::fs::read(&p.built.as_ref().expect("обновление выпущено").path).unwrap()).unwrap();
    assert_eq!((pack.manifest.story_version, pack.manifest.id.as_str()), (2, p.story.meta.id.as_str()));
    assert!(!p.dirty(), "выпущенная версия сохранена в проекте");
    let full = std::fs::read(&p.built.as_ref().unwrap().path).unwrap();
    click(&mut h, "Закрыть");

    click(&mut h, "Собрать");
    click(&mut h, "Выпустить обновление истории (.vnpak)…");
    h.get_by_label_contains("Только ресурсы").click();
    h.run();
    click(&mut h, "Выпустить");
    let p = h.state().project.as_ref().unwrap();
    assert_eq!(p.story.meta.story_version, 2, "пакет ресурсов не меняет номер выпуска");
    let assets = std::fs::read(&p.built.as_ref().expect("пакет ресурсов выпущен").path).unwrap();
    let (merged, manifest) = vn_store::pack::merge_update(full, assets).unwrap();
    assert_eq!(manifest.story_version, 2);
    assert_eq!(vn_store::pack::Pack::open(merged).unwrap().story, p.story, "игра после пакета ресурсов запускается с тем же сценарием");
}

const REPLY: &str = r#"Вот история:
```json
{"characters":[{"name":"Вера","description":"журналистка"}],
 "scenes":[
  {"key":"a","name":"Причал","lines":[{"text":"Туман."},{"speaker":"Вера","text":"Есть кто?"}],"choices":[{"text":"Ждать","goto":"b"},{"text":"Уйти","goto":"c"}]},
  {"key":"b","name":"Лодка","lines":[{"text":"Из тумана выходит лодка."}],"ending":"Встреча"},
  {"key":"c","name":"Город","lines":[{"text":"Огни города."}],"ending":"Одна"}]}
```"#;

#[test]
fn ai_draft_is_shown_first_and_added_only_on_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = with_project(tmp.path(), NewKind::Ai);
    assert_eq!(h.state().project.as_ref().unwrap().tab, Tab::Ai, "проект «с ИИ» открывается на вкладке ИИ");
    h.get_by_label_contains("Мини-инструкция");
    h.get_by_label_contains("ЛОКАЛЬНЫЙ ИИ");
    h.state_mut().ai.receive(Ok(REPLY.into()), Mode::NewStory, None);
    h.run();
    assert_eq!(story(&h).scenes.len(), 1, "до подтверждения проект не меняется");
    click(&mut h, "✔ Добавить в проект");
    let s = story(&h);
    assert_eq!(s.scenes.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["Причал", "Лодка", "Город"], "пустая стартовая сцена заменена историей");
    assert_eq!(s.meta.start_scene, Some(s.scenes[0].id.clone()));
    assert_eq!(s.characters[0].name, "Вера");
    assert!(!vn_core::validate::has_errors(&vn_core::validate::validate(s, &Default::default())), "история от ИИ проходит проверку");
    assert_eq!(vn_core::analysis::endings(s).len(), 2);

    let scene = s.scenes[0].id.clone();
    let before = s.scenes[0].events.len();
    h.state_mut().ai.receive(Ok(r#"{"scenes":[{"key":"x","name":"x","lines":[{"speaker":"Вера","text":"Подожду ещё."}]}]}"#.into()), Mode::Continue, Some(scene));
    h.run();
    h.state_mut().project.as_mut().unwrap().tab = Tab::Ai;
    h.run();
    click(&mut h, "✔ Добавить в проект");
    let events = &story(&h).scenes[0].events;
    assert_eq!(events.len(), before + 1);
    assert!(matches!(events.last().unwrap().kind, EventKind::Choice { .. }), "реплики дописаны перед выбором, а не после него");
}

#[test]
fn ai_failure_is_explained_and_changes_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = with_project(tmp.path(), NewKind::Ai);
    h.state_mut().ai.receive(Ok("Извините, не могу.".into()), Mode::NewStory, None);
    h.run();
    h.get_by_label_contains("нет JSON");
    assert!(h.state().ai.draft.is_none());
    assert_eq!(story(&h).scenes.len(), 1);
}

#[test]
fn unsaved_work_is_offered_back_after_a_crash() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("novel");
    let mut edited = project::create(&dir, "Проба").unwrap();
    edited.scenes.push(Scene::new("Несохранённая"));
    project::write_autosave(&dir, &edited).unwrap();
    let mut h = harness(tmp.path());
    h.state_mut().open_project(&dir);
    h.run();
    assert_eq!(story(&h).scenes.len(), 1, "до ответа автора открыто сохранённое");
    click(&mut h, "Восстановить");
    assert_eq!(story(&h).scenes[1].name, "Несохранённая");
    assert!(h.state().project.as_ref().unwrap().dirty());
}

#[test]
fn preview_starts_the_game_from_the_selected_scene_and_every_tab_draws() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = with_project(tmp.path(), NewKind::Demo);
    let third = story(&h).scenes[2].id.clone();
    h.state_mut().project.as_mut().unwrap().select(Some(third.clone()), None);
    h.run();
    click(&mut h, "▶ Отсюда");
    let p = h.state().project.as_ref().unwrap();
    assert_eq!(p.tab, Tab::Preview);
    assert_eq!(p.game.as_ref().unwrap().runtime().unwrap().state.scene, third);
    for tab in ["⇄ Сюжет", "Гардероб", "⚑ Концовки", "✦ ИИ", "Сцена"] {
        click(&mut h, tab);
    }
    assert!(h.state().project.as_ref().unwrap().game.is_none(), "игра не продолжает идти за пределами своей вкладки");
    for tab in ["Персонажи", "Ресурсы", "Переменные", "Проект", "Сцены"] {
        click(&mut h, tab);
    }
    click(&mut h, "⚑ Концовки");
    h.get_by_label_contains("Концовок: 4");
}

#[test]
fn hotkeys_save_undo_and_delete_the_selected_event() {
    use egui::{Key, Modifiers};
    let tmp = tempfile::tempdir().unwrap();
    let mut h = with_project(tmp.path(), NewKind::Manual);
    click(&mut h, "+ Событие");
    click(&mut h, "Концовка");
    h.step();
    assert_eq!(story(&h).scenes[0].events.len(), 1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run();
    assert!(!h.state().project.as_ref().unwrap().dirty(), "Ctrl+S сохраняет");
    h.key_press(Key::Delete);
    h.run();
    assert!(story(&h).scenes[0].events.is_empty(), "Delete удаляет выбранное событие");
    h.step();
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(story(&h).scenes[0].events.len(), 1, "Ctrl+Z возвращает его");
    assert!(tmp.path().join("novel/backups").read_dir().unwrap().flatten().any(|f| f.file_name().to_string_lossy().starts_with("backup-")), "перед первым сохранением сделана резервная копия");
}

#[test]
fn renaming_a_variable_in_the_list_updates_the_story_that_uses_it() {
    let tmp = tempfile::tempdir().unwrap();
    let mut h = with_project(tmp.path(), NewKind::Demo);
    click(&mut h, "Переменные");
    h.get_all_by_value("деньги").next().unwrap().focus();
    h.run();
    h.get_all_by_value("деньги").next().unwrap().type_text("_в_кармане");
    h.run();
    h.key_press(egui::Key::Tab);
    h.run();
    let s = story(&h);
    assert!(s.variables.iter().any(|v| v.name == "деньги_в_кармане"), "{:?}", s.variables.iter().map(|v| &v.name).collect::<Vec<_>>());
    let json = serde_json::to_string(&s.scenes).unwrap();
    assert!(json.contains("деньги_в_кармане") && !json.contains("\"деньги\"") && !json.contains("{деньги}"), "события следуют за новым именем");
    assert!(!vn_core::validate::has_errors(&vn_core::validate::validate(s, &h.state().project.as_ref().unwrap().files)), "после переименования история цела");
}
