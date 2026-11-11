//! Кнопка «Обновить редактор»: проверка выпусков на GitHub и установка нового.
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const REPO: &str = "kclazzy/visualNovel";
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
/// Окончание имени архива выпуска для этой системы.
pub const ASSET_SUFFIX: &str = if cfg!(windows) { "windows.zip" } else { "linux.zip" };

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    pub notes: String,
    pub page: String,
    /// Ссылка на архив для этой системы; `None` — в выпуске его нет.
    pub asset: Option<String>,
}

fn numbers(version: &str) -> Vec<u64> {
    version.trim().trim_start_matches(['v', 'V']).split(['.', '-', '+']).map_while(|p| p.parse().ok()).collect()
}

/// `tag` новее установленной версии `current`.
pub fn is_newer(tag: &str, current: &str) -> bool {
    let (a, b) = (numbers(tag), numbers(current));
    let n = a.len().max(b.len());
    let pad = |v: &[u64]| (0..n).map(|i| v.get(i).copied().unwrap_or(0)).collect::<Vec<_>>();
    !a.is_empty() && pad(&a) > pad(&b)
}

pub fn parse_release(json: &Value) -> Option<Release> {
    let asset = json["assets"].as_array().and_then(|a| a.iter().find(|x| x["name"].as_str().is_some_and(|n| n.ends_with(ASSET_SUFFIX)))).and_then(|x| x["browser_download_url"].as_str()).map(str::to_string);
    Some(Release { tag: json["tag_name"].as_str()?.to_string(), notes: json["body"].as_str().unwrap_or_default().to_string(), page: json["html_url"].as_str().unwrap_or_default().to_string(), asset })
}

#[derive(Clone, Debug, PartialEq)]
pub enum Check {
    UpToDate,
    Available(Release),
}

/// Решение по ответу GitHub: код ответа и тело `releases/latest`.
pub fn decide(status: u16, json: &Value, current: &str) -> Result<Check, String> {
    match status {
        200 => {
            let release = parse_release(json).ok_or("GitHub вернул непонятный ответ о выпуске.")?;
            Ok(if is_newer(&release.tag, current) { Check::Available(release) } else { Check::UpToDate })
        }
        404 => Err(format!("В репозитории {REPO} пока нет опубликованных выпусков (или он закрыт от посторонних).")),
        403 | 429 => Err("GitHub временно ограничил запросы. Попробуйте через несколько минут.".into()),
        other => Err(format!("GitHub ответил кодом {other}.")),
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(std::time::Duration::from_secs(300))).build().into()
}

pub const GITHUB_API: &str = "https://api.github.com";

/// Узнать о последнем выпуске. `api` — адрес GitHub API (в тестах — локальный сервер).
pub fn check_at(api: &str) -> Result<Check, String> {
    let url = format!("{api}/repos/{REPO}/releases/latest");
    let mut resp = agent().get(&url).header("User-Agent", "vn-studio").header("Accept", "application/vnd.github+json").call().map_err(|e| format!("Нет связи с GitHub: {e}"))?;
    let status = resp.status().as_u16();
    let json: Value = resp.body_mut().read_json().unwrap_or(Value::Null);
    decide(status, &json, CURRENT)
}

pub fn check() -> Result<Check, String> {
    check_at(GITHUB_API)
}

/// Распаковать архив выпуска в папку программы. Заменяемые файлы переименовываются в `*.old`:
/// так можно заменить работающую программу, а при сбое — вернуть прежнюю.
pub fn install_zip(bytes: &[u8], dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("Архив обновления повреждён: {e}"))?;
    let mut files = vec![];
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("Архив обновления повреждён: {e}"))?;
        let Some(rel) = entry.enclosed_name() else { return Err("В архиве обновления недопустимый путь.".into()) };
        if entry.is_dir() {
            continue;
        }
        let mut data = vec![];
        entry.read_to_end(&mut data).map_err(|e| format!("Архив обновления не читается: {e}"))?;
        files.push((rel, data));
    }
    if files.is_empty() {
        return Err("Архив обновления пуст.".into());
    }
    let io = |e: std::io::Error| format!("Не удалось записать обновление: {e}. Возможно, папка программы защищена от записи.");
    let mut written = vec![];
    for (rel, data) in files {
        let target = dir.join(&rel);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        if target.exists() {
            let mut old = target.clone().into_os_string();
            old.push(".old");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&target, &old).map_err(io)?;
        }
        std::fs::write(&target, data).map_err(io)?;
        #[cfg(unix)]
        if target.extension().is_none() {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
        }
        written.push(target);
    }
    Ok(written)
}

pub fn program_dir() -> Result<PathBuf, String> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).ok_or_else(|| "Не удалось определить папку программы.".to_string())
}

/// Скачать архив выпуска и распаковать его в папку `dir`.
pub fn install_to(release: &Release, dir: &Path) -> Result<(), String> {
    let url = release.asset.as_ref().ok_or_else(|| format!("В выпуске {} нет архива для этой системы ({ASSET_SUFFIX}). Скачайте его вручную: {}", release.tag, release.page))?;
    let mut resp = agent().get(url).header("User-Agent", "vn-studio").call().map_err(|e| format!("Не удалось скачать обновление: {e}"))?;
    if resp.status().as_u16() != 200 {
        return Err(format!("Не удалось скачать обновление: код {}", resp.status().as_u16()));
    }
    let bytes = resp.body_mut().with_config().limit(1 << 30).read_to_vec().map_err(|e| format!("Обновление скачано не полностью: {e}"))?;
    install_zip(&bytes, dir).map(|_| ())
}

pub fn install(release: &Release) -> Result<(), String> {
    install_to(release, &program_dir()?)
}

/// Запустить обновлённый редактор заново.
pub fn restart() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe).spawn().map_err(|e| format!("Не удалось перезапустить: {e}. Закройте и откройте редактор вручную."))?;
    Ok(())
}

/// Удалить `*.old`, оставшиеся от прошлого обновления.
pub fn cleanup(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.path().extension().is_some_and(|e| e == "old") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    #[test]
    fn versions_compare_by_numbers() {
        assert!(is_newer("v0.2.0", "0.1.9"));
        assert!(is_newer("v0.10.0", "0.9.0"));
        assert!(is_newer("1.0.1", "1.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1", "0.1.0"));
        assert!(!is_newer("nightly", "0.1.0"));
    }

    fn release(tag: &str) -> Value {
        json!({"tag_name": tag, "body": "Что нового", "html_url": "https://example.test/r", "assets": [
            {"name": "other.txt", "browser_download_url": "https://example.test/other"},
            {"name": format!("vn-studio-{tag}-{ASSET_SUFFIX}"), "browser_download_url": "https://example.test/zip"}]})
    }

    #[test]
    fn newer_release_is_offered_with_its_archive() {
        let Check::Available(r) = decide(200, &release("v9.0.0"), "0.1.0").unwrap() else { panic!("ожидалось обновление") };
        assert_eq!((r.tag.as_str(), r.asset.as_deref(), r.notes.as_str()), ("v9.0.0", Some("https://example.test/zip"), "Что нового"));
    }

    #[test]
    fn same_release_means_up_to_date_and_missing_releases_are_explained() {
        assert_eq!(decide(200, &release("v0.1.0"), "0.1.0").unwrap(), Check::UpToDate);
        assert!(decide(404, &Value::Null, "0.1.0").unwrap_err().contains("нет опубликованных выпусков"));
        assert!(decide(200, &json!({}), "0.1.0").is_err());
    }

    fn archive(files: &[(&str, &str)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(vec![]));
        for (name, text) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(text.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn install_replaces_files_and_keeps_the_previous_copy() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("vn-studio.exe"), "old").unwrap();
        std::fs::write(tmp.path().join("mine.txt"), "keep").unwrap();
        install_zip(&archive(&[("vn-studio.exe", "new"), ("docs/readme.txt", "hi")]), tmp.path()).unwrap();
        let read = |n: &str| std::fs::read_to_string(tmp.path().join(n)).unwrap();
        assert_eq!(read("vn-studio.exe"), "new");
        assert_eq!(read("vn-studio.exe.old"), "old");
        assert_eq!(read("docs/readme.txt"), "hi");
        assert_eq!(read("mine.txt"), "keep");
        install_zip(&archive(&[("vn-studio.exe", "newer")]), tmp.path()).unwrap();
        assert_eq!(read("vn-studio.exe.old"), "new", "повторная установка не спотыкается о старую копию");
        cleanup(tmp.path());
        assert!(!tmp.path().join("vn-studio.exe.old").exists());
    }

    #[test]
    fn broken_or_hostile_archives_change_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(install_zip(b"not a zip", tmp.path()).is_err());
        assert!(install_zip(&archive(&[]), tmp.path()).is_err());
        assert!(install_zip(&archive(&[("ok.txt", "1"), ("../evil.txt", "x")]), tmp.path()).is_err());
        assert!(!tmp.path().join("ok.txt").exists(), "при плохом архиве ничего не записано");
        assert!(!tmp.path().parent().unwrap().join("evil.txt").exists());
    }
}
