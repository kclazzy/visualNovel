//! Проверка целостности истории перед предпросмотром и выпуском.
use crate::analysis::{endings_from, reachable};
use crate::cond::Cond;
use crate::model::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Игра сломана — выпуск блокируется.
    Error,
    /// Стоит посмотреть, но выпуску не мешает.
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub severity: Severity,
    pub message: String,
    /// Куда перейти в редакторе.
    pub scene: Option<SceneId>,
    pub event: Option<EventId>,
}

struct Checker<'a> {
    story: &'a Story,
    files: &'a BTreeSet<String>,
    vars: BTreeSet<&'a str>,
    at: (Option<SceneId>, Option<EventId>),
    out: Vec<Issue>,
}

impl<'a> Checker<'a> {
    fn push(&mut self, severity: Severity, message: String) {
        self.out.push(Issue { severity, message, scene: self.at.0.clone(), event: self.at.1.clone() });
    }
    fn error(&mut self, m: String) {
        self.push(Severity::Error, m)
    }
    fn warn(&mut self, m: String) {
        self.push(Severity::Warning, m)
    }
    fn file(&mut self, path: &str, what: &str) {
        if path.is_empty() {
            self.error(format!("{what}: файл не выбран"));
        } else if !self.files.contains(path) {
            self.error(format!("{what}: файл «{path}» отсутствует в проекте"));
        }
    }
    fn var(&mut self, name: &str) {
        if !self.vars.contains(name) {
            self.warn(format!("Переменная «{name}» не определена в списке переменных"));
        }
    }
    fn cond(&mut self, cond: &Cond) {
        for v in cond.vars() {
            self.var(v);
        }
    }
    fn scene_ref(&mut self, id: &SceneId, what: &str) {
        if self.story.scene(id).is_none() {
            self.error(format!("{what} ведёт в несуществующую сцену «{id}»"));
        }
    }
    fn character(&mut self, id: &CharId, expression: Option<&str>) {
        match self.story.character(id) {
            None => self.error(format!("Персонаж «{id}» не существует")),
            Some(c) => {
                if let Some(e) = expression.filter(|e| !e.is_empty()) {
                    if !c.expressions.iter().any(|x| x.name == e) {
                        self.warn(format!("У персонажа «{}» нет выражения «{e}»", c.name));
                    }
                }
            }
        }
    }
    fn text_vars(&mut self, text: &str) {
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            let Some(len) = rest[open..].find('}') else { break };
            let name = rest[open + 1..open + len].to_string();
            self.var(&name);
            rest = &rest[open + len + 1..];
        }
    }

    fn event(&mut self, e: &Event) {
        match &e.kind {
            EventKind::Background { image, .. } => self.file(image, "Фон"),
            EventKind::ShowCharacter { character, expression, .. } | EventKind::Expression { character, expression } => self.character(character, Some(expression)),
            EventKind::HideCharacter { character, .. } | EventKind::MoveCharacter { character, .. } => self.character(character, None),
            EventKind::SetOutfit { slot, item } => {
                if !self.story.wardrobe.slots.iter().any(|s| &s.id == slot) {
                    self.error(format!("Слот одежды «{slot}» не существует"));
                }
                if let Some(id) = item {
                    match self.story.item(id) {
                        None => self.error(format!("Предмет одежды «{id}» не существует")),
                        Some(i) if &i.slot != slot => self.error(format!("Предмет «{}» не подходит к слоту «{slot}»", i.name)),
                        _ => {}
                    }
                }
            }
            EventKind::UnlockItem { item } => {
                if self.story.item(item).is_none() {
                    self.error(format!("Предмет одежды «{item}» не существует"));
                }
            }
            EventKind::OpenWardrobe { slots } => {
                if self.story.wardrobe.hero.is_none() {
                    self.error("Гардероб открыт, но главный герой не назначен".into());
                }
                for s in slots {
                    if !self.story.wardrobe.slots.iter().any(|x| &x.id == s) {
                        self.error(format!("Слот одежды «{s}» не существует"));
                    }
                }
            }
            EventKind::Say { speaker, text, expression, sound, .. } => {
                if text.trim().is_empty() {
                    self.warn("Пустой текст".into());
                }
                if let Some(s) = speaker {
                    self.character(s, expression.as_deref());
                }
                if let Some(s) = sound.as_ref().filter(|s| !s.is_empty()) {
                    self.file(s, "Звук реплики");
                }
                self.text_vars(text);
            }
            EventKind::PlayMusic { track, .. } => self.file(track, "Музыка"),
            EventKind::Sound { file, .. } => self.file(file, "Звук"),
            EventKind::Goto { scene } => self.scene_ref(scene, "Переход"),
            EventKind::Branch { cond, scene } => {
                self.cond(cond);
                self.scene_ref(scene, "Условный переход");
            }
            EventKind::Choice { options, .. } => {
                if options.is_empty() {
                    self.error("Выбор без вариантов".into());
                }
                for o in options {
                    if o.text.trim().is_empty() {
                        self.warn("Вариант выбора без текста".into());
                    }
                    if let Some(c) = &o.cond {
                        self.cond(c);
                    }
                    for op in &o.effects {
                        self.var(&op.var);
                    }
                    if let Some(to) = &o.goto {
                        self.scene_ref(to, &format!("Вариант «{}»", o.text));
                    }
                }
            }
            EventKind::SetVar(op) => self.var(&op.var),
            _ => {}
        }
    }

    fn scene(&mut self, s: &Scene) {
        if s.events.is_empty() {
            self.error(format!("Сцена «{}» пуста", s.name));
            return;
        }
        if let Some(ch) = &s.chapter {
            if !self.story.chapters.iter().any(|c| &c.id == ch) {
                self.warn(format!("Сцена «{}» относится к удалённой главе", s.name));
            }
        }
        for (i, e) in s.events.iter().enumerate() {
            self.at.1 = Some(e.id.clone());
            self.event(e);
            if e.kind.is_terminal() && i + 1 < s.events.len() {
                self.warn("События после этого никогда не выполнятся".into());
            }
        }
        let last = s.events.last().expect("непустая");
        let closed = match &last.kind {
            EventKind::Choice { options, .. } => !options.is_empty() && options.iter().all(|o| o.goto.is_some() && o.cond.is_none()),
            k => k.is_terminal(),
        };
        if !closed {
            self.error(format!("Сцена «{}» обрывается: в конце нужен переход, выбор или концовка", s.name));
        }
    }
}

/// Проверить историю. `files` — относительные пути всех файлов ресурсов проекта.
pub fn validate(story: &Story, files: &BTreeSet<String>) -> Vec<Issue> {
    let mut c = Checker { story, files, vars: story.variables.iter().map(|v| v.name.as_str()).collect(), at: (None, None), out: vec![] };

    match &story.meta.start_scene {
        None => c.error("Не выбрана начальная сцена".into()),
        Some(id) => c.scene_ref(id, "Начало истории"),
    }
    if let Some(id) = &story.meta.fallback_scene {
        c.scene_ref(id, "Запасная сцена для старых сохранений");
    }
    for (from, to) in &story.redirects {
        if story.scene(to).is_none() && !story.redirects.contains_key(to) {
            c.warn(format!("Перенаправление сохранений из удалённой сцены «{from}» ведёт в несуществующую сцену"));
        }
    }

    let mut seen: BTreeMap<&str, u32> = BTreeMap::new();
    for id in story.scenes.iter().map(|s| s.id.as_str()).chain(story.scenes.iter().flat_map(|s| &s.events).map(|e| e.id.as_str())) {
        *seen.entry(id).or_default() += 1;
    }
    for (id, n) in seen {
        if id.is_empty() {
            c.error("Найдена сцена или событие без ID".into());
        } else if n > 1 {
            c.error(format!("ID «{id}» используется {n} раза — сохранения не смогут найти место"));
        }
    }

    for ch in &story.characters {
        for e in &ch.expressions {
            c.file(&e.image, &format!("Персонаж «{}», выражение «{}»", ch.name, e.name));
        }
    }
    let w = &story.wardrobe;
    if let Some(h) = &w.hero {
        if story.character(h).is_none() {
            c.error("Главный герой гардероба не существует".into());
        }
    }
    for i in &w.items {
        c.file(&i.image, &format!("Одежда «{}»", i.name));
        if !w.slots.iter().any(|s| s.id == i.slot) {
            c.error(format!("Одежда «{}» привязана к несуществующему слоту", i.name));
        }
    }

    for s in &story.scenes {
        c.at = (Some(s.id.clone()), None);
        c.scene(s);
    }

    if let Some(start) = story.meta.start_scene.as_ref().filter(|s| story.scene(s).is_some()) {
        let live = reachable(story, start);
        for s in &story.scenes {
            c.at = (Some(s.id.clone()), None);
            if !live.contains(&s.id) {
                c.warn(format!("В сцену «{}» невозможно попасть", s.name));
            } else if endings_from(story, &s.id).is_empty() {
                c.warn(format!("Из сцены «{}» нельзя дойти ни до одной концовки (замкнутый круг?)", s.name));
            }
        }
    }

    c.at = (None, None);
    let used = story.assets_used();
    for f in files.difference(&used) {
        c.warn(format!("Файл «{f}» не используется"));
    }
    c.out.sort_by_key(|i| i.severity);
    c.out
}

pub fn has_errors(issues: &[Issue]) -> bool {
    issues.iter().any(|i| i.severity == Severity::Error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::tests::{branching, say, scene, story};

    fn messages(story: &Story, files: &[&str]) -> Vec<(Severity, String)> {
        validate(story, &files.iter().map(|f| f.to_string()).collect()).into_iter().map(|i| (i.severity, i.message)).collect()
    }

    fn has(list: &[(Severity, String)], sev: Severity, part: &str) -> bool {
        list.iter().any(|(s, m)| *s == sev && m.contains(part))
    }

    #[test]
    fn healthy_story_has_no_issues() {
        assert_eq!(messages(&branching(), &[]), vec![]);
    }

    #[test]
    fn deleted_scene_referenced_by_choice_is_an_error_pointing_at_the_event() {
        let mut s = branching();
        s.scenes.retain(|sc| sc.id.as_str() != "help");
        let issues = validate(&s, &BTreeSet::new());
        let hit = issues.iter().find(|i| i.message.contains("Вариант «Помочь» ведёт в несуществующую сцену")).expect("ошибка есть");
        assert_eq!(hit.severity, Severity::Error);
        assert_eq!(hit.scene, Some("start".into()));
        assert_eq!(hit.event, Some(s.scenes[0].events[1].id.clone()));
        assert!(has_errors(&issues));
    }

    #[test]
    fn missing_and_unused_files() {
        let mut s = branching();
        s.scenes[0].events.insert(0, Event::new(EventKind::Background { image: "assets/backgrounds/room.png".into(), fade: 0.0 }));
        s.scenes[0].events.insert(0, Event::new(EventKind::PlayMusic { track: String::new(), volume: 1.0, fade: 0.0, looped: true }));
        let m = messages(&s, &["assets/backgrounds/old.png"]);
        assert!(has(&m, Severity::Error, "«assets/backgrounds/room.png» отсутствует"));
        assert!(has(&m, Severity::Error, "Музыка: файл не выбран"));
        assert!(has(&m, Severity::Warning, "«assets/backgrounds/old.png» не используется"));
    }

    #[test]
    fn flow_problems() {
        let mut s = story(vec![
            scene("a", vec![say("раз"), Event::new(EventKind::Goto { scene: "loop".into() }), say("мёртвый код")]),
            scene("loop", vec![say(""), Event::new(EventKind::Goto { scene: "loop".into() })]),
            scene("island", vec![say("никто не придёт")]),
            scene("empty", vec![]),
        ]);
        s.scenes[0].events[0].id = "dup".into();
        s.scenes[1].events[0].id = "dup".into();
        let m = messages(&s, &[]);
        assert!(has(&m, Severity::Warning, "никогда не выполнятся"));
        assert!(has(&m, Severity::Warning, "Пустой текст"));
        assert!(has(&m, Severity::Warning, "В сцену «island» невозможно попасть"));
        assert!(has(&m, Severity::Error, "Сцена «island» обрывается"));
        assert!(has(&m, Severity::Error, "Сцена «empty» пуста"));
        assert!(has(&m, Severity::Warning, "Из сцены «loop» нельзя дойти ни до одной концовки"));
        assert!(has(&m, Severity::Error, "ID «dup» используется 2 раза"));
        assert!(m.windows(2).all(|w| w[0].0 <= w[1].0), "ошибки идут первыми");
    }

    #[test]
    fn undefined_variables_characters_and_clothes() {
        let mut s = branching();
        s.variables.clear();
        s.scenes[0].events.insert(0, Event::new(EventKind::Say { speaker: Some("ghost".into()), style: TextStyle::Dialogue, text: "Привет, {name}".into(), expression: None, speed: None, sound: None }));
        s.scenes[0].events.insert(0, Event::new(EventKind::SetOutfit { slot: "top".into(), item: Some("coat".into()) }));
        let m = messages(&s, &[]);
        assert!(has(&m, Severity::Warning, "Переменная «name» не определена"));
        assert!(has(&m, Severity::Warning, "Переменная «money» не определена"));
        assert!(has(&m, Severity::Warning, "Переменная «trust» не определена"));
        assert!(has(&m, Severity::Error, "Персонаж «ghost» не существует"));
        assert!(has(&m, Severity::Error, "Слот одежды «top» не существует"));
        assert!(has(&m, Severity::Error, "Предмет одежды «coat» не существует"));
    }

    #[test]
    fn missing_start_scene() {
        let mut s = branching();
        s.meta.start_scene = None;
        assert!(has(&messages(&s, &[]), Severity::Error, "Не выбрана начальная сцена"));
    }
}
