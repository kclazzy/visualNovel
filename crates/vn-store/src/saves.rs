//! Сохранения и настройки игрока. Лежат в пользовательском каталоге данных,
//! отдельно от файлов игры: обновление и переустановка их не трогают.
use crate::fsutil::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use vn_core::save::{parse_save, SaveError, SaveFile};

pub const SLOTS: usize = 8;
pub const AUTO: &str = "auto";
pub const QUICK: &str = "quick";

pub fn slot_name(n: usize) -> String {
    format!("slot{n}")
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct PlayerSettings {
    /// Множитель скорости текста.
    pub text_speed: f32,
    pub music: f32,
    pub sfx: f32,
    pub ui: f32,
    pub fullscreen: bool,
    /// Задержка автоперехода к следующей реплике, секунд.
    pub auto_delay: f32,
}

impl Default for PlayerSettings {
    fn default() -> Self {
        Self { text_speed: 1.0, music: 0.7, sfx: 0.9, ui: 0.6, fullscreen: false, auto_delay: 1.5 }
    }
}

#[derive(Clone, Debug)]
pub struct SaveStore {
    dir: PathBuf,
}

impl SaveStore {
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Каталог сохранений истории в данных пользователя (`%APPDATA%\VNStudio\saves\<id>` в Windows).
    pub fn for_story(story_id: &str) -> Self {
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        Self::at(base.join("VNStudio").join("saves").join(safe_name(story_id)))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, slot: &str) -> PathBuf {
        self.dir.join(format!("{}.json", safe_name(slot)))
    }

    /// Записать сохранение; прежнее содержимое слота остаётся в `.bak`.
    pub fn write(&self, slot: &str, save: &SaveFile) -> Result<(), String> {
        let path = self.path(slot);
        let e = |e: std::io::Error| format!("Не удалось сохранить игру: {e}");
        if path.exists() {
            fs::copy(&path, path.with_extension("bak")).map_err(e)?;
        }
        write_json(&path, save).map_err(e)
    }

    /// Прочитать слот. Если файл повреждён, берётся его предыдущая копия.
    pub fn read(&self, slot: &str) -> Option<Result<SaveFile, SaveError>> {
        let path = self.path(slot);
        let main = fs::read_to_string(&path).ok().map(|t| parse_save(&t));
        match main {
            Some(Ok(s)) => Some(Ok(s)),
            other => match fs::read_to_string(path.with_extension("bak")).ok().map(|t| parse_save(&t)) {
                Some(Ok(s)) => Some(Ok(s)),
                _ => other,
            },
        }
    }

    pub fn delete(&self, slot: &str) {
        let _ = fs::remove_file(self.path(slot));
        let _ = fs::remove_file(self.path(slot).with_extension("bak"));
    }

    /// Самое свежее сохранение среди всех слотов — для кнопки «Продолжить».
    pub fn latest(&self) -> Option<SaveFile> {
        let names = [AUTO.to_string(), QUICK.to_string()].into_iter().chain((1..=SLOTS).map(slot_name));
        names.filter_map(|n| self.read(&n)?.ok()).max_by_key(|s| s.saved_at)
    }

    /// Копия всех сохранений перед установкой обновления истории.
    pub fn backup(&self, tag: &str) -> Result<Option<PathBuf>, String> {
        if !self.dir.exists() {
            return Ok(None);
        }
        let target = self.dir.join("backups").join(format!("{}-{}", stamp(now()), safe_name(tag)));
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        for entry in fs::read_dir(&self.dir).map_err(|e| e.to_string())?.flatten() {
            if entry.path().is_file() {
                fs::copy(entry.path(), target.join(entry.file_name())).map_err(|e| e.to_string())?;
            }
        }
        Ok(Some(target))
    }

    pub fn settings(&self) -> PlayerSettings {
        fs::read_to_string(self.dir.join("settings.cfg")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn write_settings(&self, s: &PlayerSettings) -> Result<(), String> {
        write_json(&self.dir.join("settings.cfg"), s).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vn_core::model::Story;
    use vn_core::runtime::Runtime;

    fn save_at(time: u64) -> SaveFile {
        Runtime::new(Story::new("t")).save(time)
    }

    #[test]
    fn slots_round_trip_and_latest_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SaveStore::at(tmp.path().join("saves"));
        assert!(store.read(QUICK).is_none());
        assert!(store.latest().is_none());
        store.write(&slot_name(1), &save_at(10)).unwrap();
        store.write(AUTO, &save_at(30)).unwrap();
        store.write(QUICK, &save_at(20)).unwrap();
        assert_eq!(store.read(&slot_name(1)).unwrap().unwrap().saved_at, 10);
        assert_eq!(store.latest().unwrap().saved_at, 30);
        store.delete(AUTO);
        assert_eq!(store.latest().unwrap().saved_at, 20);
    }

    #[test]
    fn corrupt_slot_falls_back_to_previous_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SaveStore::at(tmp.path().to_path_buf());
        store.write(QUICK, &save_at(1)).unwrap();
        store.write(QUICK, &save_at(2)).unwrap();
        fs::write(store.path(QUICK), "мусор").unwrap();
        assert_eq!(store.read(QUICK).unwrap().unwrap().saved_at, 1);
        fs::write(store.path(QUICK).with_extension("bak"), "мусор").unwrap();
        assert!(matches!(store.read(QUICK), Some(Err(SaveError::Corrupt(_)))));
    }

    #[test]
    fn backup_copies_saves_and_settings_persist() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SaveStore::at(tmp.path().join("s"));
        assert_eq!(store.backup("v2").unwrap(), None);
        assert_eq!(store.settings(), PlayerSettings::default());
        store.write(AUTO, &save_at(5)).unwrap();
        let s = PlayerSettings { music: 0.1, fullscreen: true, ..Default::default() };
        store.write_settings(&s).unwrap();
        assert_eq!(store.settings(), s);
        let copy = store.backup("выпуск 2").unwrap().unwrap();
        assert!(copy.join("auto.json").exists());
        assert_eq!(store.latest().unwrap().saved_at, 5, "настройки и копии не мешают слотам");
    }
}
