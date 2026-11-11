use std::fs;
use std::io;
use std::path::Path;

/// Записать файл целиком: сначала во временный, затем атомарно переименовать.
/// Сбой посреди записи не портит прежнее содержимое.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("")));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> io::Result<()> {
    write_atomic(path, serde_json::to_string_pretty(value).map_err(io::Error::other)?.as_bytes())
}

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Дата и время UTC из секунд Unix: (год, месяц, день, час, минута, секунда).
pub fn civil(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = (secs % 86_400) as u32;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day, rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Метка времени для имён файлов: `20261111-134501`.
pub fn stamp(secs: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(secs);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// Дата для показа человеку: `11.11.2026 13:45`.
pub fn human_time(secs: u64) -> String {
    let (y, mo, d, h, mi, _) = civil(secs);
    format!("{d:02}.{mo:02}.{y:04} {h:02}:{mi:02}")
}

/// Имя, безопасное для файла и URL: буквы, цифры, `_`, `-`, `.`.
pub fn safe_name(name: &str) -> String {
    let s: String = name.trim().chars().map(|c| if c.is_alphanumeric() || matches!(c, '_' | '-' | '.') { c } else { '_' }).collect();
    let s = s.trim_matches('.').to_string();
    if s.is_empty() { "novel".into() } else { s }
}

pub fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(stamp(1_794_404_701), "20261111-134501");
        assert_eq!(human_time(951_782_400), "29.02.2000 00:00");
    }

    #[test]
    fn names_are_sanitized() {
        assert_eq!(safe_name("Моя новелла: часть 1/2"), "Моя_новелла__часть_1_2");
        assert_eq!(safe_name("../.."), "_", "выход из папки невозможен");
        assert_eq!(safe_name("  "), "novel");
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a/b.json");
        write_atomic(&p, b"1").unwrap();
        write_atomic(&p, b"2").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"2");
        assert_eq!(fs::read_dir(p.parent().unwrap()).unwrap().count(), 1);
    }
}
