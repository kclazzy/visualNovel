//! Правки истории, которые делает редактор. Чистые функции: интерфейс только вызывает их.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use vn_core::model::*;

pub fn asset_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn short(text: &str, max: usize) -> String {
    let line = text.replace('\n', " ");
    if line.chars().count() > max {
        format!("{}…", line.chars().take(max).collect::<String>())
    } else {
        line
    }
}

fn char_name<'a>(story: &'a Story, id: &CharId) -> &'a str {
    story.character(id).map(|c| c.name.as_str()).unwrap_or("?")
}

fn scene_name<'a>(story: &'a Story, id: &SceneId) -> &'a str {
    story.scene(id).map(|s| s.name.as_str()).unwrap_or("несуществующая сцена")
}

/// Короткое описание события для таймлайна.
pub fn summary(story: &Story, kind: &EventKind) -> String {
    match kind {
        EventKind::Background { image, .. } => format!("Фон: {}", asset_name(image)),
        EventKind::ShowCharacter { character, .. } => format!("Показать: {}", char_name(story, character)),
        EventKind::HideCharacter { character, .. } => format!("Скрыть: {}", char_name(story, character)),
        EventKind::MoveCharacter { character, .. } => format!("Переместить: {}", char_name(story, character)),
        EventKind::Expression { character, expression } => format!("{}: {expression}", char_name(story, character)),
        EventKind::SetOutfit { item, .. } => format!("Одежда: {}", item.as_ref().and_then(|i| story.item(i)).map(|i| i.name.as_str()).unwrap_or("снять")),
        EventKind::UnlockItem { item } => format!("Открыть предмет: {}", story.item(item).map(|i| i.name.as_str()).unwrap_or("?")),
        EventKind::OpenWardrobe { .. } => "Открыть гардероб".into(),
        EventKind::Say { speaker, text, .. } => match speaker {
            Some(s) => format!("{}: {}", char_name(story, s), short(text, 60)),
            None => short(text, 70),
        },
        EventKind::DialogBox { pos } => format!("Окно диалога: {}", match pos {
            DialogPos::Top => "сверху",
            DialogPos::Center => "по центру",
            DialogPos::Bottom => "снизу",
        }),
        EventKind::PlayMusic { track, .. } => format!("Музыка: {}", asset_name(track)),
        EventKind::StopMusic { .. } => "Остановить музыку".into(),
        EventKind::MusicVolume { volume, .. } => format!("Громкость музыки: {:.0}%", volume * 100.0),
        EventKind::Sound { file, .. } => format!("Звук: {}", asset_name(file)),
        EventKind::Wait { seconds } => format!("Пауза {seconds:.1} с"),
        EventKind::Fade { out: true, .. } => "Затемнение экрана".into(),
        EventKind::Fade { out: false, .. } => "Проявление экрана".into(),
        EventKind::Goto { scene } => format!("→ {}", scene_name(story, scene)),
        EventKind::Choice { options, .. } => format!("Выбор: {}", options.iter().map(|o| short(&o.text, 18)).collect::<Vec<_>>().join(" / ")),
        EventKind::SetVar(op) => format!("{} {} {}", op.var, match op.action {
            VarAction::Set => "=",
            VarAction::Add => "+=",
            VarAction::Sub => "−=",
        }, op.value),
        EventKind::Branch { cond, scene } => format!("Если {cond} → {}", scene_name(story, scene)),
        EventKind::End { title } => format!("Концовка: {title}"),
    }
}

/// Заготовки событий для меню «Добавить событие».
pub fn palette(story: &Story, scene: &SceneId) -> Vec<EventKind> {
    let ch = story.characters.first().map(|c| c.id.clone()).unwrap_or_default();
    let expr = story.characters.first().and_then(|c| c.expressions.first()).map(|e| e.name.clone()).unwrap_or_default();
    let other = story.scenes.iter().map(|s| &s.id).find(|id| *id != scene).unwrap_or(scene).clone();
    let var = story.variables.first().map(|v| v.name.clone()).unwrap_or_default();
    let slot = story.wardrobe.slots.first().map(|s| s.id.clone()).unwrap_or_default();
    let item = story.wardrobe.items.first().map(|i| i.id.clone()).unwrap_or_default();
    vec![
        EventKind::Say { speaker: story.characters.first().map(|c| c.id.clone()), style: TextStyle::Dialogue, text: String::new(), expression: None, speed: None, sound: None },
        EventKind::Say { speaker: None, style: TextStyle::Narration, text: String::new(), expression: None, speed: None, sound: None },
        EventKind::Background { image: String::new(), fade: 0.5 },
        EventKind::ShowCharacter { character: ch.clone(), expression: expr.clone(), pos: Pos::CENTER, fade: 0.3 },
        EventKind::HideCharacter { character: ch.clone(), fade: 0.3 },
        EventKind::MoveCharacter { character: ch.clone(), pos: Pos::LEFT, duration: 0.4 },
        EventKind::Expression { character: ch, expression: expr },
        EventKind::Choice { prompt: String::new(), options: vec![ChoiceOption::new("Первый вариант", None), ChoiceOption::new("Второй вариант", None)] },
        EventKind::Goto { scene: other.clone() },
        EventKind::Branch { cond: vn_core::cond::Cond::Cmp { var: var.clone(), cmp: vn_core::cond::Cmp::Ge, value: Value::Int(1) }, scene: other },
        EventKind::SetVar(VarOp { var, action: VarAction::Add, value: Value::Int(1) }),
        EventKind::PlayMusic { track: String::new(), volume: 0.8, fade: 1.0, looped: true },
        EventKind::StopMusic { fade: 1.0 },
        EventKind::MusicVolume { volume: 0.4, fade: 1.0 },
        EventKind::Sound { file: String::new(), volume: 1.0 },
        EventKind::Wait { seconds: 1.0 },
        EventKind::Fade { out: true, duration: 0.6 },
        EventKind::Fade { out: false, duration: 0.6 },
        EventKind::DialogBox { pos: DialogPos::Bottom },
        EventKind::SetOutfit { slot, item: Some(item.clone()) },
        EventKind::UnlockItem { item },
        EventKind::OpenWardrobe { slots: vec![] },
        EventKind::End { title: "Конец".into() },
    ]
}

/// Название пункта меню для заготовки (два вида текста различаются).
pub fn palette_title(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Say { speaker: Some(_), .. } => "Реплика персонажа",
        EventKind::Say { .. } => "Текст автора",
        other => other.title(),
    }
}

/// Вставить события после позиции `after` (или в конец). Возвращает ID первого вставленного.
pub fn insert_events(scene: &mut Scene, after: Option<usize>, events: Vec<Event>) -> Option<EventId> {
    let first = events.first().map(|e| e.id.clone());
    let at = after.map(|i| (i + 1).min(scene.events.len())).unwrap_or(scene.events.len());
    scene.events.splice(at..at, events);
    first
}

/// Копия события с новыми ID (самого события и вариантов выбора).
pub fn clone_event(event: &Event) -> Event {
    let mut e = event.clone();
    e.id = EventId::new("ev");
    if let EventKind::Choice { options, .. } = &mut e.kind {
        for o in options {
            o.id = OptionId::new("op");
        }
    }
    e
}

/// Соединить сцены переходом `from → to` так, как это сделал бы автор вручную.
pub fn link_scenes(story: &mut Story, from: &SceneId, to: &SceneId) -> Result<(), String> {
    let to_name = scene_name(story, to).to_string();
    let names: BTreeMap<SceneId, String> = story.scenes.iter().map(|s| (s.id.clone(), s.name.clone())).collect();
    let scene = story.scene_mut(from).ok_or("Сцена не найдена")?;
    match scene.events.last_mut().map(|e| &mut e.kind) {
        Some(EventKind::Choice { options, .. }) => options.push(ChoiceOption::new(&to_name, Some(to.clone()))),
        Some(EventKind::End { .. }) => return Err("Сцена заканчивается концовкой. Удалите событие «Концовка», чтобы добавить переход.".into()),
        Some(kind @ EventKind::Goto { .. }) => {
            let EventKind::Goto { scene: old } = kind.clone() else { unreachable!() };
            if &old == to {
                return Err("Такой переход уже есть.".into());
            }
            let old_name = names.get(&old).cloned().unwrap_or_else(|| "Вариант".into());
            *kind = EventKind::Choice { prompt: String::new(), options: vec![ChoiceOption::new(&old_name, Some(old)), ChoiceOption::new(&to_name, Some(to.clone()))] };
        }
        _ => scene.events.push(Event::new(EventKind::Goto { scene: to.clone() })),
    }
    Ok(())
}

pub const NODE: [f32; 2] = [190.0, 64.0];

/// Расставить узлы графа, у которых ещё нет места: колонками по удалению от начала истории.
pub fn auto_layout(story: &mut Story, all: bool) {
    if all {
        story.layout.clear();
    }
    let mut next: BTreeMap<SceneId, Vec<SceneId>> = BTreeMap::new();
    for e in story.edges() {
        next.entry(e.from).or_default().push(e.to);
    }
    let mut depth: BTreeMap<SceneId, usize> = BTreeMap::new();
    let mut queue: VecDeque<SceneId> = story.meta.start_scene.iter().cloned().collect();
    if let Some(s) = queue.front() {
        depth.insert(s.clone(), 0);
    }
    while let Some(id) = queue.pop_front() {
        let d = depth[&id];
        for to in next.get(&id).into_iter().flatten() {
            if story.scene(to).is_some() && !depth.contains_key(to) {
                depth.insert(to.clone(), d + 1);
                queue.push_back(to.clone());
            }
        }
    }
    let orphan_column = depth.values().max().map(|m| m + 1).unwrap_or(0);
    let taken: BTreeSet<(i32, i32)> = story.layout.values().map(|p| ((p[0] / (NODE[0] + 70.0)).round() as i32, (p[1] / (NODE[1] + 36.0)).round() as i32)).collect();
    let mut taken = taken;
    for s in &story.scenes {
        if story.layout.contains_key(&s.id) {
            continue;
        }
        let col = depth.get(&s.id).copied().unwrap_or(orphan_column) as i32;
        let row = (0..).find(|r| !taken.contains(&(col, *r))).unwrap_or(0);
        taken.insert((col, row));
        story.layout.insert(s.id.clone(), [col as f32 * (NODE[0] + 70.0), row as f32 * (NODE[1] + 36.0)]);
    }
}

/// Изменить положение персонажа, каким оно видно после события `upto`:
/// правится последнее событие «показать/переместить» этого персонажа.
pub fn place_character(scene: &mut Scene, upto: usize, ch: &CharId, change: impl FnOnce(&mut Pos)) -> bool {
    let found = scene.events.iter_mut().take(upto + 1).rev().find_map(|e| match &mut e.kind {
        EventKind::ShowCharacter { character, pos, .. } | EventKind::MoveCharacter { character, pos, .. } if character == ch => Some(pos),
        _ => None,
    });
    match found {
        Some(pos) => {
            change(pos);
            pos.x = pos.x.clamp(0.0, 1.0);
            pos.y = pos.y.clamp(0.2, 1.6);
            pos.scale = pos.scale.clamp(0.2, 3.0);
            true
        }
        None => false,
    }
}

/// Подготовить историю к выпуску обновления: новый номер выпуска и версия.
pub fn bump_release(story: &mut Story, version: &str) {
    story.meta.story_version += 1;
    if !version.trim().is_empty() {
        story.meta.version = version.trim().to_string();
    }
}

/// Убрать ссылки на удалённый файл ресурса.
pub fn forget_asset(story: &mut Story, path: &str) {
    for c in &mut story.characters {
        c.expressions.retain(|e| e.image != path);
    }
    story.wardrobe.items.retain(|i| i.image != path);
    for e in story.scenes.iter_mut().flat_map(|s| &mut s.events) {
        match &mut e.kind {
            EventKind::Background { image: f, .. } | EventKind::PlayMusic { track: f, .. } | EventKind::Sound { file: f, .. } if f == path => f.clear(),
            EventKind::Say { sound, .. } if sound.as_deref() == Some(path) => *sound = None,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn story3() -> (Story, SceneId, SceneId, SceneId) {
        let mut s = Story::new("t");
        let a = s.scenes[0].id.clone();
        let (b, c) = (Scene::new("Б"), Scene::new("В"));
        let (bid, cid) = (b.id.clone(), c.id.clone());
        s.scenes.extend([b, c]);
        (s, a, bid, cid)
    }

    #[test]
    fn linking_grows_from_goto_to_choice() {
        let (mut s, a, b, c) = story3();
        link_scenes(&mut s, &a, &b).unwrap();
        assert!(matches!(&s.scenes[0].events[0].kind, EventKind::Goto { scene } if scene == &b));
        assert!(link_scenes(&mut s, &a, &b).is_err(), "повторный переход не дублируется");
        link_scenes(&mut s, &a, &c).unwrap();
        let EventKind::Choice { options, .. } = &s.scenes[0].events[0].kind else { panic!("ожидался выбор") };
        assert_eq!(options.iter().map(|o| (o.text.as_str(), o.goto.clone())).collect::<Vec<_>>(), vec![("Б", Some(b.clone())), ("В", Some(c))]);
        link_scenes(&mut s, &a, &a).unwrap();
        assert_eq!(s.edges().len(), 3);
        s.scene_mut(&b).unwrap().events.push(Event::new(EventKind::End { title: "x".into() }));
        assert!(link_scenes(&mut s, &b, &a).is_err());
    }

    #[test]
    fn layout_puts_scenes_in_columns_by_distance_without_overlap() {
        let (mut s, a, b, c) = story3();
        link_scenes(&mut s, &a, &b).unwrap();
        link_scenes(&mut s, &a, &c).unwrap();
        let lonely = Scene::new("Одна");
        let lid = lonely.id.clone();
        s.scenes.push(lonely);
        auto_layout(&mut s, false);
        assert_eq!(s.layout[&a], [0.0, 0.0]);
        assert_eq!(s.layout[&b][0], s.layout[&c][0]);
        assert!(s.layout[&b][0] > 0.0 && s.layout[&b][1] != s.layout[&c][1]);
        assert!(s.layout[&lid][0] > s.layout[&b][0], "несвязанная сцена — в отдельной колонке");
        s.layout.insert(a.clone(), [500.0, 500.0]);
        auto_layout(&mut s, false);
        assert_eq!(s.layout[&a], [500.0, 500.0], "расставленное автором не трогаем");
    }

    #[test]
    fn dragging_edits_the_latest_placement_before_the_selected_event() {
        let mut sc = Scene::new("s");
        let ch = CharId::from("anna");
        sc.events = vec![
            Event::new(EventKind::ShowCharacter { character: ch.clone(), expression: String::new(), pos: Pos::LEFT, fade: 0.0 }),
            Event::new(EventKind::MoveCharacter { character: ch.clone(), pos: Pos::CENTER, duration: 0.0 }),
            Event::new(EventKind::MoveCharacter { character: ch.clone(), pos: Pos::RIGHT, duration: 0.0 }),
        ];
        assert!(place_character(&mut sc, 1, &ch, |p| p.x = 5.0));
        let pos = |i: usize| match &sc.events[i].kind {
            EventKind::ShowCharacter { pos, .. } | EventKind::MoveCharacter { pos, .. } => *pos,
            _ => unreachable!(),
        };
        assert_eq!((pos(0), pos(1).x, pos(2)), (Pos::LEFT, 1.0, Pos::RIGHT));
        assert!(!place_character(&mut sc, 2, &CharId::from("max"), |_| {}));
    }

    #[test]
    fn inserting_and_cloning_events() {
        let mut sc = Scene::new("s");
        let mk = |t: &str| Event::new(EventKind::End { title: t.into() });
        insert_events(&mut sc, None, vec![mk("a"), mk("c")]);
        let id = insert_events(&mut sc, Some(0), vec![mk("b")]).unwrap();
        assert_eq!(sc.event_index(&id), Some(1));
        let choice = Event::new(EventKind::Choice { prompt: String::new(), options: vec![ChoiceOption::new("x", None)] });
        let copy = clone_event(&choice);
        let (EventKind::Choice { options: a, .. }, EventKind::Choice { options: b, .. }) = (&choice.kind, &copy.kind) else { unreachable!() };
        assert!(copy.id != choice.id && a[0].id != b[0].id && a[0].text == b[0].text);
    }

    #[test]
    fn palette_and_summaries_cover_every_event_type() {
        let (mut s, a, ..) = story3();
        s.characters.push(Character::new("Анна"));
        let kinds = palette(&s, &a);
        let titles: BTreeSet<&str> = kinds.iter().map(palette_title).collect();
        assert_eq!(titles.len(), kinds.len(), "пункты меню не повторяются");
        assert!(kinds.iter().all(|k| !summary(&s, k).is_empty()));
        assert_eq!(summary(&s, &EventKind::Goto { scene: "nope".into() }), "→ несуществующая сцена");
    }

    #[test]
    fn release_bump_and_asset_removal() {
        let (mut s, ..) = story3();
        bump_release(&mut s, " 1.1 ");
        assert_eq!((s.meta.story_version, s.meta.version.as_str()), (2, "1.1"));
        bump_release(&mut s, "");
        assert_eq!((s.meta.story_version, s.meta.version.as_str()), (3, "1.1"));
        s.scenes[0].events.push(Event::new(EventKind::Background { image: "assets/backgrounds/a.png".into(), fade: 0.0 }));
        forget_asset(&mut s, "assets/backgrounds/a.png");
        assert!(s.assets_used().is_empty());
    }
}
