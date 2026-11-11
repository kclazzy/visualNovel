//! Настройки редактора на этом компьютере: недавние проекты и подключение ИИ.
//! Лежат в папке настроек пользователя, а не в проекте — ключ API не попадает в игру.
use crate::ai::AiSettings;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub recent: Vec<PathBuf>,
    pub ai: AiSettings,
}

pub fn default_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("VNStudio").join("studio.json")
}

/// Папка, в которой по умолчанию создаются проекты.
pub fn projects_dir() -> PathBuf {
    dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from(".")).join("VisualNovels")
}

impl AppConfig {
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }
    pub fn save(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = vn_store::fsutil::write_json(path, self);
    }
    pub fn remember(&mut self, project: &Path) {
        self.recent.retain(|p| p != project);
        self.recent.insert(0, project.to_path_buf());
        self.recent.truncate(8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_survives_restart_and_tolerates_garbage() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cfg/studio.json");
        let mut c = AppConfig::default();
        c.ai.model = "m".into();
        c.remember(Path::new("/a"));
        c.remember(Path::new("/b"));
        c.remember(Path::new("/a"));
        c.save(&path);
        let back = AppConfig::load(&path);
        assert_eq!(back, c);
        assert_eq!(back.recent, vec![PathBuf::from("/a"), PathBuf::from("/b")]);
        std::fs::write(&path, "{broken").unwrap();
        assert_eq!(AppConfig::load(&path), AppConfig::default());
    }
}
