//! Пакет истории `.vnpak`: один файл со сценарием, версией и ресурсами.
//! Игра обновляется заменой этого файла; сохранения лежат отдельно.
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::sync::Mutex;
use vn_core::model::{Story, FORMAT_VERSION};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub story_version: u32,
    pub format_version: u32,
    /// `false` — пакет несёт только сценарий; ресурсы остаются от установленной версии.
    pub has_assets: bool,
    /// `false` — пакет несёт только ресурсы; сценарий остаётся от установленной версии.
    #[serde(default = "yes")]
    pub has_story: bool,
}

fn yes() -> bool {
    true
}

type Zip = ZipArchive<Cursor<Vec<u8>>>;

fn zerr(e: impl std::fmt::Display) -> String {
    format!("Ошибка пакета истории: {e}")
}

fn put(zip: &mut ZipWriter<Cursor<Vec<u8>>>, name: &str, bytes: &[u8], compress: bool) -> Result<(), String> {
    let method = if compress { CompressionMethod::Deflated } else { CompressionMethod::Stored };
    zip.start_file(name, SimpleFileOptions::default().compression_method(method)).map_err(zerr)?;
    zip.write_all(bytes).map_err(zerr)
}

fn manifest_of(story: &Story, has_assets: bool, has_story: bool) -> Manifest {
    Manifest { id: story.meta.id.clone(), name: story.meta.name.clone(), version: story.meta.version.clone(), story_version: story.meta.story_version, format_version: story.meta.format_version, has_assets, has_story }
}

fn put_manifest(zip: &mut ZipWriter<Cursor<Vec<u8>>>, m: &Manifest) -> Result<(), String> {
    put(zip, "manifest.json", serde_json::to_string_pretty(m).map_err(zerr)?.as_bytes(), true)
}

fn put_header(zip: &mut ZipWriter<Cursor<Vec<u8>>>, story: &Story, has_assets: bool) -> Result<(), String> {
    put_manifest(zip, &manifest_of(story, has_assets, true))?;
    put(zip, "story.json", serde_json::to_string(story).map_err(zerr)?.as_bytes(), true)
}

fn put_assets(zip: &mut ZipWriter<Cursor<Vec<u8>>>, story: &Story, dir: &Path) -> Result<(), String> {
    for asset in story.assets_used() {
        let bytes = std::fs::read(dir.join(&asset)).map_err(|e| format!("Ресурс «{asset}» не прочитан: {e}"))?;
        put(zip, &asset, &bytes, false)?;
    }
    Ok(())
}

/// Собрать пакет только с ресурсами: перерисованные картинки, новая музыка. Сценарий в него не входит.
pub fn build_assets(story: &Story, project: &Path) -> Result<Vec<u8>, String> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    put_manifest(&mut zip, &manifest_of(story, true, false))?;
    put_assets(&mut zip, story, project)?;
    Ok(zip.finish().map_err(zerr)?.into_inner())
}

/// Собрать пакет. `project` — папка проекта, из которой берутся используемые ресурсы;
/// `None` — пакет только со сценарием.
/// Собрать пакет. `project` — папка проекта, из которой берутся используемые ресурсы;
/// `None` — пакет только со сценарием.
pub fn build(story: &Story, project: Option<&Path>) -> Result<Vec<u8>, String> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    put_header(&mut zip, story, project.is_some())?;
    if let Some(dir) = project {
        put_assets(&mut zip, story, dir)?;
    }
    Ok(zip.finish().map_err(zerr)?.into_inner())
}

fn read_entry(zip: &mut Zip, name: &str) -> Option<Vec<u8>> {
    let mut file = zip.by_name(name).ok()?;
    let mut out = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut out).ok()?;
    Some(out)
}

/// Открытый пакет: сценарий разобран, ресурсы читаются по требованию.
pub struct Pack {
    pub manifest: Manifest,
    pub story: Story,
    zip: Mutex<Zip>,
}

impl Pack {
    pub fn open(bytes: Vec<u8>) -> Result<Pack, String> {
        let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Файл не является пакетом истории (.vnpak) или повреждён".to_string())?;
        let manifest: Manifest = serde_json::from_slice(&read_entry(&mut zip, "manifest.json").ok_or("В пакете нет manifest.json")?).map_err(zerr)?;
        if manifest.format_version > FORMAT_VERSION {
            return Err(format!("Пакет истории создан для более новой версии движка (формат {}). Обновите игру целиком.", manifest.format_version));
        }
        if !manifest.has_story {
            return Err("В этом пакете только ресурсы: он устанавливается как обновление поверх игры".into());
        }
        let story: Story = serde_json::from_slice(&read_entry(&mut zip, "story.json").ok_or("В пакете нет story.json")?).map_err(zerr)?;
        Ok(Pack { manifest, story, zip: Mutex::new(zip) })
    }

    pub fn asset(&self, path: &str) -> Option<Vec<u8>> {
        let mut zip = self.zip.lock().ok()?;
        read_entry(&mut zip, path)
    }

    fn asset_names(&self) -> Vec<String> {
        self.zip.lock().map(|z| z.file_names().filter(|n| n.starts_with("assets/")).map(str::to_string).collect()).unwrap_or_default()
    }
}

fn assets_of(zip: &mut Zip) -> Vec<(String, Vec<u8>)> {
    let names: Vec<String> = zip.file_names().filter(|n| n.starts_with("assets/")).map(str::to_string).collect();
    names.into_iter().filter_map(|n| read_entry(zip, &n).map(|b| (n, b))).collect()
}

/// Наложить пакет ресурсов: файлы из него заменяют одноимённые, сценарий и версия остаются.
/// `None` — обновление не является пакетом ресурсов.
fn merge_assets(installed: &[u8], update: &[u8]) -> Option<Result<(Vec<u8>, Manifest), String>> {
    let mut new = ZipArchive::new(Cursor::new(update.to_vec())).ok()?;
    let manifest: Manifest = serde_json::from_slice(&read_entry(&mut new, "manifest.json")?).ok()?;
    if manifest.has_story {
        return None;
    }
    Some((|| {
        let old = Pack::open(installed.to_vec())?;
        if manifest.id != old.manifest.id {
            return Err(format!("Обновление предназначено для другой истории («{}»)", manifest.name));
        }
        let fresh = assets_of(&mut new);
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        put_header(&mut zip, &old.story, true)?;
        for name in old.asset_names() {
            if !fresh.iter().any(|(n, _)| *n == name) {
                if let Some(bytes) = old.asset(&name) {
                    put(&mut zip, &name, &bytes, false)?;
                }
            }
        }
        for (name, bytes) in &fresh {
            put(&mut zip, name, bytes, false)?;
        }
        Ok((zip.finish().map_err(zerr)?.into_inner(), Manifest { has_assets: true, ..old.manifest.clone() }))
    })())
}

/// Наложить обновление на установленный пакет. Возвращает новый пакет целиком.
/// Обновление от другой истории или старее установленного отвергается.
pub fn merge_update(installed: Vec<u8>, update: Vec<u8>) -> Result<(Vec<u8>, Manifest), String> {
    if let Some(result) = merge_assets(&installed, &update) {
        return result;
    }
    let new = Pack::open(update.clone())?;
    let old = Pack::open(installed)?;
    if new.manifest.id != old.manifest.id {
        return Err(format!("Обновление предназначено для другой истории («{}»)", new.manifest.name));
    }
    if new.manifest.story_version < old.manifest.story_version {
        return Err(format!("Обновление (выпуск {}) старее установленной версии (выпуск {})", new.manifest.story_version, old.manifest.story_version));
    }
    if new.manifest.has_assets {
        return Ok((update, new.manifest));
    }
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    put_header(&mut zip, &new.story, old.manifest.has_assets)?;
    for name in old.asset_names() {
        if let Some(bytes) = old.asset(&name) {
            put(&mut zip, &name, &bytes, false)?;
        }
    }
    let manifest = Manifest { has_assets: old.manifest.has_assets, ..new.manifest };
    Ok((zip.finish().map_err(zerr)?.into_inner(), manifest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vn_core::model::*;

    fn project() -> (tempfile::TempDir, Story) {
        let tmp = tempfile::tempdir().unwrap();
        let mut story = crate::project::create(tmp.path(), "Пакет").unwrap();
        std::fs::write(tmp.path().join("assets/backgrounds/room.png"), b"PNGDATA").unwrap();
        std::fs::write(tmp.path().join("assets/backgrounds/unused.png"), b"X").unwrap();
        story.scenes[0].events = vec![Event::new(EventKind::Background { image: "assets/backgrounds/room.png".into(), fade: 0.0 }), Event::new(EventKind::End { title: "Конец".into() })];
        (tmp, story)
    }

    #[test]
    fn pack_carries_story_and_only_used_assets() {
        let (tmp, story) = project();
        let pack = Pack::open(build(&story, Some(tmp.path())).unwrap()).unwrap();
        assert_eq!(pack.story, story);
        assert_eq!(pack.manifest.story_version, 1);
        assert!(pack.manifest.has_assets);
        assert_eq!(pack.asset("assets/backgrounds/room.png").unwrap(), b"PNGDATA");
        assert!(pack.asset("assets/backgrounds/unused.png").is_none());
    }

    #[test]
    fn missing_asset_fails_the_build_and_garbage_is_not_a_pack() {
        let (tmp, mut story) = project();
        story.scenes[0].events.insert(0, Event::new(EventKind::Sound { file: "assets/sounds/none.wav".into(), volume: 1.0 }));
        assert!(build(&story, Some(tmp.path())).unwrap_err().contains("none.wav"));
        assert!(Pack::open(b"not a zip".to_vec()).is_err());
    }

    #[test]
    fn script_only_update_keeps_installed_assets() {
        let (tmp, mut story) = project();
        let installed = build(&story, Some(tmp.path())).unwrap();
        story.meta.story_version = 2;
        story.meta.version = "1.1".into();
        story.scenes[0].name = "Переименована".into();
        let (merged, manifest) = merge_update(installed, build(&story, None).unwrap()).unwrap();
        let pack = Pack::open(merged).unwrap();
        assert_eq!((manifest.story_version, manifest.has_assets), (2, true));
        assert_eq!(pack.manifest, manifest);
        assert_eq!(pack.story.scenes[0].name, "Переименована");
        assert_eq!(pack.asset("assets/backgrounds/room.png").unwrap(), b"PNGDATA");
    }

    #[test]
    fn assets_only_update_swaps_files_and_leaves_the_story_alone() {
        let (tmp, mut story) = project();
        let installed = build(&story, Some(tmp.path())).unwrap();
        std::fs::write(tmp.path().join("assets/backgrounds/room.png"), b"REDRAWN").unwrap();
        story.scenes[0].name = "Правка, которая не должна попасть к игроку".into();
        let update = build_assets(&story, tmp.path()).unwrap();
        assert!(Pack::open(update.clone()).err().unwrap().contains("только ресурсы"), "такой пакет нельзя принять за игру");
        let (merged, manifest) = merge_update(installed.clone(), update.clone()).unwrap();
        let pack = Pack::open(merged).unwrap();
        assert_eq!(pack.asset("assets/backgrounds/room.png").unwrap(), b"REDRAWN");
        assert_eq!(pack.story, Pack::open(installed.clone()).unwrap().story, "сценарий прежний");
        assert_eq!((manifest.story_version, manifest.has_assets), (1, true));
        story.meta.id = "other".into();
        assert!(merge_update(installed, build_assets(&story, tmp.path()).unwrap()).unwrap_err().contains("другой истории"));
    }

    #[test]
    fn full_update_replaces_everything() {
        let (tmp, mut story) = project();
        let installed = build(&story, Some(tmp.path())).unwrap();
        std::fs::write(tmp.path().join("assets/backgrounds/room.png"), b"NEWPNG").unwrap();
        story.meta.story_version = 2;
        let (merged, _) = merge_update(installed, build(&story, Some(tmp.path())).unwrap()).unwrap();
        assert_eq!(Pack::open(merged).unwrap().asset("assets/backgrounds/room.png").unwrap(), b"NEWPNG");
    }

    #[test]
    fn foreign_older_and_too_new_updates_are_rejected() {
        let (tmp, mut story) = project();
        story.meta.story_version = 5;
        let installed = build(&story, Some(tmp.path())).unwrap();
        story.meta.story_version = 4;
        assert!(merge_update(installed.clone(), build(&story, None).unwrap()).unwrap_err().contains("старее"));
        story.meta.story_version = 6;
        story.meta.id = "other".into();
        assert!(merge_update(installed.clone(), build(&story, None).unwrap()).unwrap_err().contains("другой истории"));
        story.meta.format_version = FORMAT_VERSION + 1;
        assert!(merge_update(installed, build(&story, None).unwrap()).unwrap_err().contains("более новой версии движка"));
    }
}
