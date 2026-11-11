//! Папка проекта: автономная, с относительными путями, по файлу на сцену.
//! Повреждение одного файла сцены не губит весь проект.
use crate::fsutil::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use vn_core::model::*;

pub const FOLDERS: [&str; 14] = [
    "story/scenes", "story/characters", "story/branches", "story/variables", "assets/backgrounds", "assets/characters", "assets/clothing", "assets/music", "assets/sounds", "assets/ui", "previews", "backups", "builds", "changelog",
];

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    #[serde(flatten)]
    meta: Meta,
    #[serde(default)]
    chapters: Vec<Chapter>,
    /// Порядок сцен; сами сцены лежат в `story/scenes/<id>.json`.
    #[serde(default)]
    scenes: Vec<SceneId>,
    #[serde(default)]
    redirects: BTreeMap<SceneId, SceneId>,
    /// Ресурсы, которые использует история (справочно, пересчитывается при сохранении).
    #[serde(default)]
    assets: BTreeSet<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct CastFile {
    #[serde(default)]
    characters: Vec<Character>,
    #[serde(default)]
    wardrobe: Wardrobe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssetKind {
    Background,
    Character,
    Clothing,
    Music,
    Sound,
    Ui,
}

impl AssetKind {
    pub const ALL: [AssetKind; 6] = [AssetKind::Background, AssetKind::Character, AssetKind::Clothing, AssetKind::Music, AssetKind::Sound, AssetKind::Ui];
    pub fn folder(self) -> &'static str {
        match self {
            AssetKind::Background => "assets/backgrounds",
            AssetKind::Character => "assets/characters",
            AssetKind::Clothing => "assets/clothing",
            AssetKind::Music => "assets/music",
            AssetKind::Sound => "assets/sounds",
            AssetKind::Ui => "assets/ui",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            AssetKind::Background => "Фоны",
            AssetKind::Character => "Спрайты",
            AssetKind::Clothing => "Одежда",
            AssetKind::Music => "Музыка",
            AssetKind::Sound => "Звуки",
            AssetKind::Ui => "Интерфейс",
        }
    }
    pub fn is_audio(self) -> bool {
        matches!(self, AssetKind::Music | AssetKind::Sound)
    }
    pub fn extensions(self) -> &'static [&'static str] {
        if self.is_audio() { &["mp3", "wav", "ogg"] } else { &["png", "jpg", "jpeg", "webp"] }
    }
    /// Тип ресурса по его пути внутри проекта.
    pub fn of(path: &str) -> Option<AssetKind> {
        Self::ALL.into_iter().find(|k| path.starts_with(&format!("{}/", k.folder())))
    }
}

fn scene_path(dir: &Path, id: &SceneId) -> PathBuf {
    dir.join("story/scenes").join(format!("{}.json", safe_name(id.as_str())))
}

fn err<E: std::fmt::Display>(what: &str) -> impl Fn(E) -> String + '_ {
    move |e| format!("{what}: {e}")
}

/// Создать новый проект в пустой или несуществующей папке.
pub fn create(dir: &Path, name: &str) -> Result<Story, String> {
    if dir.join("project.json").exists() {
        return Err("В этой папке уже есть проект".into());
    }
    let story = Story::new(name);
    save(dir, &story)?;
    Ok(story)
}

/// Сохранить проект. Файлы удалённых сцен переносятся в `backups/trash`, а не стираются.
pub fn save(dir: &Path, story: &Story) -> Result<(), String> {
    let io = err::<std::io::Error>("Не удалось сохранить проект");
    for f in FOLDERS {
        fs::create_dir_all(dir.join(f)).map_err(&io)?;
    }
    for s in &story.scenes {
        write_json(&scene_path(dir, &s.id), s).map_err(&io)?;
    }
    write_json(&dir.join("story/characters/characters.json"), &CastFile { characters: story.characters.clone(), wardrobe: story.wardrobe.clone() }).map_err(&io)?;
    write_json(&dir.join("story/variables/variables.json"), &story.variables).map_err(&io)?;
    write_json(&dir.join("story/branches/layout.json"), &story.layout).map_err(&io)?;
    let file = ProjectFile { meta: story.meta.clone(), chapters: story.chapters.clone(), scenes: story.scenes.iter().map(|s| s.id.clone()).collect(), redirects: story.redirects.clone(), assets: story.assets_used() };
    let main = dir.join("project.json");
    if main.exists() {
        fs::copy(&main, dir.join("backups/project.json.bak")).map_err(&io)?;
    }
    write_json(&main, &file).map_err(&io)?;

    let keep: BTreeSet<PathBuf> = story.scenes.iter().map(|s| scene_path(dir, &s.id)).collect();
    for entry in fs::read_dir(dir.join("story/scenes")).map_err(&io)?.flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "json") && !keep.contains(&p) {
            let trash = dir.join("backups/trash");
            fs::create_dir_all(&trash).map_err(&io)?;
            fs::rename(&p, trash.join(entry.file_name())).map_err(&io)?;
        }
    }
    clear_autosave(dir);
    Ok(())
}

pub struct Loaded {
    pub story: Story,
    /// Что не удалось прочитать. Проект при этом открыт с тем, что уцелело.
    pub problems: Vec<String>,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn read_or_default<T: serde::de::DeserializeOwned + Default>(path: &Path, what: &str, problems: &mut Vec<String>) -> T {
    if !path.exists() {
        return T::default();
    }
    read_json(path).unwrap_or_else(|e| {
        problems.push(format!("{what} не прочитаны ({e}). Использованы пустые значения."));
        T::default()
    })
}

/// Открыть проект. Ошибка — только если не читается сам `project.json` и его резервная копия.
pub fn load(dir: &Path) -> Result<Loaded, String> {
    let mut problems = vec![];
    let file: ProjectFile = match read_json(&dir.join("project.json")) {
        Ok(f) => f,
        Err(e) => {
            let bak: ProjectFile = read_json(&dir.join("backups/project.json.bak")).map_err(|_| format!("Файл project.json повреждён или отсутствует: {e}"))?;
            problems.push(format!("project.json повреждён ({e}); открыта его резервная копия."));
            bak
        }
    };
    if file.meta.format_version > FORMAT_VERSION {
        return Err(format!("Проект создан более новой версией редактора (формат {}). Обновите редактор.", file.meta.format_version));
    }
    let mut scenes = vec![];
    let mut listed = BTreeSet::new();
    for id in &file.scenes {
        listed.insert(scene_path(dir, id));
        match read_json::<Scene>(&scene_path(dir, id)) {
            Ok(s) => scenes.push(s),
            Err(e) => problems.push(format!("Сцена «{id}» не прочитана ({e}) и пропущена. Её файл остался в story/scenes — восстановите его из backups.")),
        }
    }
    if let Ok(entries) = fs::read_dir(dir.join("story/scenes")) {
        let mut extra: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json") && !listed.contains(p)).collect();
        extra.sort();
        for p in extra {
            if let Ok(s) = read_json::<Scene>(&p) {
                problems.push(format!("Найдена сцена «{}», не записанная в project.json — добавлена в конец списка.", s.name));
                scenes.push(s);
            }
        }
    }
    let cast: CastFile = read_or_default(&dir.join("story/characters/characters.json"), "Персонажи и гардероб", &mut problems);
    let story = Story {
        meta: file.meta,
        chapters: file.chapters,
        scenes,
        characters: cast.characters,
        variables: read_or_default(&dir.join("story/variables/variables.json"), "Переменные", &mut problems),
        wardrobe: cast.wardrobe,
        redirects: file.redirects,
        layout: read_or_default(&dir.join("story/branches/layout.json"), "Раскладка сюжетного графа", &mut problems),
    };
    Ok(Loaded { story, problems })
}

fn walk(dir: &Path, rel: &str, out: &mut BTreeSet<String>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = format!("{rel}/{}", entry.file_name().to_string_lossy());
        if entry.path().is_dir() {
            walk(&entry.path(), &name, out);
        } else {
            out.insert(name);
        }
    }
}

/// Относительные пути всех файлов ресурсов проекта.
pub fn list_assets(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    walk(&dir.join("assets"), "assets", &mut out);
    out
}

/// Скопировать файл в проект. Возвращает относительный путь ресурса.
pub fn import_asset(dir: &Path, kind: AssetKind, src: &Path) -> Result<String, String> {
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !kind.extensions().contains(&ext.as_str()) {
        return Err(format!("«{}»: для раздела «{}» подходят файлы {}", src.display(), kind.title(), kind.extensions().join(", ")));
    }
    let folder = dir.join(kind.folder());
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let stem = safe_name(src.file_stem().and_then(|s| s.to_str()).unwrap_or("file"));
    if src.parent().and_then(|p| p.canonicalize().ok()) == folder.canonicalize().ok() {
        return Ok(format!("{}/{}", kind.folder(), src.file_name().unwrap_or_default().to_string_lossy()));
    }
    let name = (0..).map(|n| if n == 0 { format!("{stem}.{ext}") } else { format!("{stem}_{n}.{ext}") }).find(|n| !folder.join(n).exists()).expect("бесконечный перебор");
    fs::copy(src, folder.join(&name)).map_err(|e| format!("Не удалось скопировать «{}»: {e}", src.display()))?;
    Ok(format!("{}/{name}", kind.folder()))
}

/// Раздел ресурсов, подходящий файлу по расширению (для перетаскивания в окно).
pub fn guess_kind(path: &Path, images_as: AssetKind) -> Option<AssetKind> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    if AssetKind::Music.extensions().contains(&ext.as_str()) {
        Some(if images_as.is_audio() { images_as } else { AssetKind::Music })
    } else if AssetKind::Background.extensions().contains(&ext.as_str()) {
        Some(if images_as.is_audio() { AssetKind::Background } else { images_as })
    } else {
        None
    }
}

const BACKUPS_KEPT: usize = 20;

/// Резервная копия сценария (без ресурсов) в `backups/backup-<время>.zip`.
pub fn backup(dir: &Path) -> Result<PathBuf, String> {
    use std::io::Write;
    let e = err::<std::io::Error>("Резервная копия не создана");
    let ze = err::<zip::result::ZipError>("Резервная копия не создана");
    let mut files = BTreeSet::new();
    walk(&dir.join("story"), "story", &mut files);
    files.insert("project.json".into());
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for f in files {
        if let Ok(bytes) = fs::read(dir.join(&f)) {
            zip.start_file(f, zip::write::SimpleFileOptions::default()).map_err(&ze)?;
            zip.write_all(&bytes).map_err(&e)?;
        }
    }
    let bytes = zip.finish().map_err(&ze)?.into_inner();
    let folder = dir.join("backups");
    let path = folder.join(format!("backup-{}.zip", stamp(now())));
    write_atomic(&path, &bytes).map_err(&e)?;
    let mut old: Vec<PathBuf> = fs::read_dir(&folder).map_err(&e)?.flatten().map(|x| x.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("backup-"))).collect();
    old.sort();
    for p in old.iter().rev().skip(BACKUPS_KEPT) {
        let _ = fs::remove_file(p);
    }
    Ok(path)
}

fn autosave_path(dir: &Path) -> PathBuf {
    dir.join("backups/autosave.json")
}

/// Снимок несохранённой работы — на случай аварийного завершения редактора.
pub fn write_autosave(dir: &Path, story: &Story) -> Result<(), String> {
    write_atomic(&autosave_path(dir), serde_json::to_string(story).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())
}

/// Несохранённая работа, оставшаяся после сбоя.
pub fn read_autosave(dir: &Path) -> Option<Story> {
    read_json(&autosave_path(dir)).ok()
}

pub fn clear_autosave(dir: &Path) {
    let _ = fs::remove_file(autosave_path(dir));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(dir: &Path) -> Story {
        let mut story = create(dir, "Проба").unwrap();
        let mut second = Scene::new("Вторая");
        second.events.push(Event::new(EventKind::End { title: "Конец".into() }));
        let first = story.scenes[0].id.clone();
        story.scenes[0].events.push(Event::new(EventKind::Goto { scene: second.id.clone() }));
        story.layout.insert(first, [10.0, 20.0]);
        story.scenes.push(second);
        story.characters.push(Character::new("Анна"));
        story.variables.push(VarDef { name: "trust".into(), initial: Value::Int(1), description: String::new() });
        story.chapters.push(Chapter { id: "c1".into(), name: "Глава 1".into() });
        save(dir, &story).unwrap();
        story
    }

    #[test]
    fn project_round_trips_through_its_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let story = sample(tmp.path());
        let loaded = load(tmp.path()).unwrap();
        assert_eq!(loaded.story, story);
        assert!(loaded.problems.is_empty());
        for f in FOLDERS {
            assert!(tmp.path().join(f).is_dir(), "{f}");
        }
        assert!(create(tmp.path(), "ещё").is_err(), "нельзя создать проект поверх существующего");
    }

    #[test]
    fn project_survives_being_moved() {
        let tmp = tempfile::tempdir().unwrap();
        let story = sample(&tmp.path().join("a"));
        fs::rename(tmp.path().join("a"), tmp.path().join("b")).unwrap();
        assert_eq!(load(&tmp.path().join("b")).unwrap().story, story);
        let text = fs::read_to_string(tmp.path().join("b/project.json")).unwrap();
        assert!(!text.contains(tmp.path().to_str().unwrap()), "в проекте нет абсолютных путей");
    }

    #[test]
    fn one_corrupt_scene_does_not_lose_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        let story = sample(tmp.path());
        fs::write(scene_path(tmp.path(), &story.scenes[1].id), "{ обрыв").unwrap();
        let loaded = load(tmp.path()).unwrap();
        assert_eq!(loaded.story.scenes.len(), 1);
        assert_eq!(loaded.story.characters, story.characters);
        assert!(loaded.problems[0].contains("не прочитана"));
    }

    #[test]
    fn corrupt_project_file_falls_back_to_its_backup() {
        let tmp = tempfile::tempdir().unwrap();
        let story = sample(tmp.path());
        save(tmp.path(), &story).unwrap();
        fs::write(tmp.path().join("project.json"), "").unwrap();
        let loaded = load(tmp.path()).unwrap();
        assert_eq!(loaded.story.scenes.len(), 2);
        assert!(loaded.problems[0].contains("резервная копия"));
        fs::remove_file(tmp.path().join("backups/project.json.bak")).unwrap();
        assert!(load(tmp.path()).is_err());
    }

    #[test]
    fn newer_format_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut story = sample(tmp.path());
        story.meta.format_version = FORMAT_VERSION + 1;
        save(tmp.path(), &story).unwrap();
        assert!(load(tmp.path()).err().unwrap().contains("более новой версией"));
    }

    #[test]
    fn deleted_scene_file_goes_to_trash_and_orphan_is_recovered() {
        let tmp = tempfile::tempdir().unwrap();
        let mut story = sample(tmp.path());
        let gone = story.scenes[1].id.clone();
        story.remove_scene(&gone, None);
        save(tmp.path(), &story).unwrap();
        assert!(!scene_path(tmp.path(), &gone).exists());
        let trashed = tmp.path().join("backups/trash").join(format!("{gone}.json"));
        assert!(trashed.exists());
        fs::rename(trashed, scene_path(tmp.path(), &gone)).unwrap();
        let loaded = load(tmp.path()).unwrap();
        assert_eq!(loaded.story.scenes.len(), 2, "сцена, вернувшаяся в папку, подхвачена");
    }

    #[test]
    fn assets_are_copied_in_with_unique_names_and_checked_by_type() {
        let tmp = tempfile::tempdir().unwrap();
        sample(tmp.path());
        let outside = tmp.path().join("мой фон (1).PNG");
        fs::write(&outside, b"png").unwrap();
        let a = import_asset(tmp.path(), AssetKind::Background, &outside).unwrap();
        let b = import_asset(tmp.path(), AssetKind::Background, &outside).unwrap();
        assert_eq!(a, "assets/backgrounds/мой_фон__1_.png");
        assert_eq!(b, "assets/backgrounds/мой_фон__1__1.png");
        assert!(import_asset(tmp.path(), AssetKind::Music, &outside).is_err());
        let again = import_asset(tmp.path(), AssetKind::Background, &tmp.path().join(&a)).unwrap();
        assert_eq!(again, a, "файл уже в проекте — не дублируется");
        assert_eq!(list_assets(tmp.path()).into_iter().collect::<Vec<_>>(), vec![a.clone(), b]);
        assert_eq!(AssetKind::of(&a), Some(AssetKind::Background));
        assert_eq!(guess_kind(Path::new("x.ogg"), AssetKind::Background), Some(AssetKind::Music));
        assert_eq!(guess_kind(Path::new("x.jpg"), AssetKind::Clothing), Some(AssetKind::Clothing));
        assert_eq!(guess_kind(Path::new("x.txt"), AssetKind::Clothing), None);
    }

    #[test]
    fn backup_and_autosave() {
        let tmp = tempfile::tempdir().unwrap();
        let mut story = sample(tmp.path());
        let zip_path = backup(tmp.path()).unwrap();
        let mut zip = zip::ZipArchive::new(fs::File::open(zip_path).unwrap()).unwrap();
        assert!(zip.by_name("project.json").is_ok());
        assert!(zip.file_names().any(|n| n.starts_with("story/scenes/")));

        assert!(read_autosave(tmp.path()).is_none());
        story.meta.name = "Несохранённое".into();
        write_autosave(tmp.path(), &story).unwrap();
        assert_eq!(read_autosave(tmp.path()).unwrap().meta.name, "Несохранённое");
        save(tmp.path(), &story).unwrap();
        assert!(read_autosave(tmp.path()).is_none(), "обычное сохранение снимает автосохранение");
    }
}
