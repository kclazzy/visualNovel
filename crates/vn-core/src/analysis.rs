//! Анализ сюжета: достижимость сцен и разбор концовок.
use crate::model::*;
use std::collections::{BTreeMap, BTreeSet};

fn successors(story: &Story) -> BTreeMap<SceneId, Vec<SceneId>> {
    let mut map: BTreeMap<SceneId, Vec<SceneId>> = BTreeMap::new();
    for e in story.edges() {
        map.entry(e.from).or_default().push(e.to);
    }
    map
}

fn walk(next: &BTreeMap<SceneId, Vec<SceneId>>, from: &SceneId) -> BTreeSet<SceneId> {
    let mut seen = BTreeSet::from([from.clone()]);
    let mut todo = vec![from.clone()];
    while let Some(s) = todo.pop() {
        for t in next.get(&s).into_iter().flatten() {
            if seen.insert(t.clone()) {
                todo.push(t.clone());
            }
        }
    }
    seen
}

/// Существующие сцены, в которые можно попасть из `from` (включая её саму).
pub fn reachable(story: &Story, from: &SceneId) -> BTreeSet<SceneId> {
    walk(&successors(story), from).into_iter().filter(|s| story.scene(s).is_some()).collect()
}

fn endings_in(story: &Story, scenes: &BTreeSet<SceneId>) -> BTreeSet<EventId> {
    story.scenes.iter().filter(|s| scenes.contains(&s.id)).flat_map(|s| &s.events).filter(|e| matches!(e.kind, EventKind::End { .. })).map(|e| e.id.clone()).collect()
}

/// Концовки, до которых можно дойти из сцены.
pub fn endings_from(story: &Story, from: &SceneId) -> BTreeSet<EventId> {
    endings_in(story, &reachable(story, from))
}

/// Вариант выбора, от которого зависит, откроется ли концовка.
#[derive(Clone, Debug, PartialEq)]
pub struct Decisive {
    pub scene: SceneId,
    pub event: EventId,
    pub option: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ending {
    pub scene: SceneId,
    pub event: EventId,
    pub title: String,
    /// Можно ли дойти до концовки от начала истории.
    pub reachable: bool,
    /// Сцены, через которые можно прийти к концовке.
    pub leading: Vec<SceneId>,
    /// Выборы, которые нужно сделать именно так, чтобы концовка осталась достижимой.
    pub decisive: Vec<Decisive>,
}

/// Все концовки истории с путями к ним.
pub fn endings(story: &Story) -> Vec<Ending> {
    let next = successors(story);
    let reach: BTreeMap<&SceneId, BTreeSet<SceneId>> = story.scenes.iter().map(|s| (&s.id, walk(&next, &s.id))).collect();
    let live = story.meta.start_scene.as_ref().and_then(|s| reach.get(s)).cloned().unwrap_or_default();
    let mut out = vec![];
    for sc in &story.scenes {
        for ev in &sc.events {
            let EventKind::End { title } = &ev.kind else { continue };
            let leading: Vec<SceneId> = story.scenes.iter().filter(|s| live.contains(&s.id) && reach[&s.id].contains(&sc.id)).map(|s| s.id.clone()).collect();
            let mut decisive = vec![];
            for s in story.scenes.iter().filter(|s| leading.contains(&s.id)) {
                for (i, e) in s.events.iter().enumerate() {
                    let EventKind::Choice { options, .. } = &e.kind else { continue };
                    let via = |to: &SceneId| reach.get(to).is_some_and(|r| r.contains(&sc.id));
                    // Вариант без перехода продолжает сцену: считаются только события после выбора.
                    let stays = s.events[i + 1..].iter().any(|later| later.id == ev.id) || story.edges().iter().any(|x| x.from == s.id && s.event_index(&x.event).is_some_and(|at| at > i) && via(&x.to));
                    let keeps: Vec<bool> = options.iter().map(|o| o.goto.as_ref().map_or(stays, via)).collect();
                    if keeps.iter().any(|k| *k) && !keeps.iter().all(|k| *k) {
                        decisive.extend(options.iter().zip(&keeps).filter(|(_, k)| **k).map(|(o, _)| Decisive { scene: s.id.clone(), event: e.id.clone(), option: o.text.clone() }));
                    }
                }
            }
            out.push(Ending { scene: sc.id.clone(), event: ev.id.clone(), title: title.clone(), reachable: live.contains(&sc.id), leading, decisive });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::tests::{branching, say, scene};

    #[test]
    fn reachability_ignores_missing_targets() {
        let mut s = branching();
        s.scenes.retain(|sc| sc.id.as_str() != "bribe");
        assert_eq!(reachable(&s, &"start".into()).into_iter().map(|s| s.0).collect::<Vec<_>>(), vec!["help", "start"]);
    }

    #[test]
    fn endings_know_their_paths_and_decisive_choices() {
        let mut s = branching();
        s.scenes.push(scene("secret", vec![say("…"), Event::new(EventKind::End { title: "Тайная".into() })]));
        let list = endings(&s);
        assert_eq!(list.iter().map(|e| (e.title.as_str(), e.reachable)).collect::<Vec<_>>(), vec![("Одиночество", true), ("Дружба", true), ("Сделка", true), ("Тайная", false)]);
        let deal = &list[2];
        assert_eq!(deal.leading, vec![SceneId::from("start"), SceneId::from("bribe")]);
        assert_eq!(deal.decisive.iter().map(|d| d.option.as_str()).collect::<Vec<_>>(), vec!["Подкупить"]);
        let alone = &list[0];
        assert_eq!(alone.decisive.iter().map(|d| d.option.as_str()).collect::<Vec<_>>(), vec!["Промолчать"], "остаться в сцене — тоже решение");
        assert!(list[3].leading.is_empty());
    }
}
