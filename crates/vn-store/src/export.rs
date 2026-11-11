//! Сборка игр: папка с игрой для Windows, пакет обновления и веб-версия для телефона.
use crate::fsutil::*;
use crate::pack::{self, Manifest};
use crate::saves::SaveStore;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use vn_core::model::Story;

pub const PACK_FILE: &str = "game_data/story.vnpak";

fn io(what: &str) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!("{what}: {e}")
}

fn readme(story: &Story, exe: &str) -> String {
    format!(
        "{name}\r\nВерсия {version}{author}\r\n\r\n\
Запуск: {exe}\r\n\r\n\
Управление: щелчок / пробел / Enter — дальше, Esc — меню, F5 — быстрое сохранение,\r\n\
F9 — быстрая загрузка, Ctrl — пропуск прочитанного, H — скрыть интерфейс, F11 — полный экран.\r\n\r\n\
Сохранения хранятся отдельно от игры, в папке данных пользователя\r\n\
(Windows: %APPDATA%\\VNStudio\\saves\\{id}). Удаление или обновление игры их не затрагивает.\r\n\r\n\
Как установить обновление истории: положите файл обновления (.vnpak) в эту папку,\r\n\
рядом с {exe}, и запустите игру. Игра сделает копию сохранений, обновит историю\r\n\
и продолжит с сохранённого места.\r\n",
        name = story.meta.name,
        version = story.meta.version,
        author = if story.meta.author.is_empty() { String::new() } else { format!(" — {}", story.meta.author) },
        id = safe_name(&story.meta.id),
    )
}

/// Собрать самостоятельную игру в `builds/<имя>/`. `player` — исполняемый файл плеера.
pub fn export_desktop(project: &Path, story: &Story, player: &Path) -> Result<PathBuf, String> {
    let name = safe_name(&story.meta.name);
    let out = project.join("builds").join(&name);
    let e = io("Не удалось собрать игру");
    fs::create_dir_all(out.join("config")).map_err(&e)?;
    let exe = match player.extension().and_then(|x| x.to_str()) {
        Some(ext) => format!("{name}.{ext}"),
        None => name.clone(),
    };
    fs::copy(player, out.join(&exe)).map_err(|err| format!("Не найден плеер «{}»: {err}. Он должен лежать рядом с редактором.", player.display()))?;
    write_atomic(&out.join(PACK_FILE), &pack::build(story, Some(project))?).map_err(&e)?;
    write_json(&out.join("config/game.json"), &serde_json::json!({ "fullscreen": story.meta.fullscreen, "resolution": story.meta.resolution })).map_err(&e)?;
    write_atomic(&out.join("README.txt"), readme(story, &exe).as_bytes()).map_err(&e)?;
    Ok(out)
}

/// Выпустить файл обновления `builds/<имя>-update-<выпуск>.vnpak`.
/// `with_assets = false` — только сценарий (маленький файл; ресурсы остаются прежними).
pub fn export_update(project: &Path, story: &Story, with_assets: bool) -> Result<PathBuf, String> {
    let bytes = pack::build(story, with_assets.then_some(project))?;
    let path = project.join("builds").join(format!("{}-update-{}.vnpak", safe_name(&story.meta.name), story.meta.story_version));
    write_atomic(&path, &bytes).map_err(io("Не удалось записать обновление"))?;
    let log = project.join("changelog").join(format!("release-{}.txt", story.meta.story_version));
    let line = format!("{} — выпуск {} (версия {}), {}\n", human_time(now()), story.meta.story_version, story.meta.version, if with_assets { "полное обновление" } else { "только сценарий" });
    write_atomic(&log, line.as_bytes()).map_err(io("Не удалось записать журнал выпусков"))?;
    Ok(path)
}

/// Что произошло при запуске игры с файлами обновлений рядом.
#[derive(Debug, PartialEq)]
pub enum UpdateOutcome {
    Applied(Manifest),
    Rejected { file: String, reason: String },
}

/// Установить обновления (`*.vnpak` в папке игры). Перед заменой истории копируются сохранения.
/// Отвергнутое обновление игру не меняет.
pub fn apply_updates(game: &Path, saves: &dyn Fn(&str) -> SaveStore) -> Vec<UpdateOutcome> {
    let mut files: Vec<PathBuf> = fs::read_dir(game).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "vnpak")).collect();
    files.sort();
    files
        .into_iter()
        .map(|file| {
            let name = file.file_name().unwrap_or_default().to_string_lossy().to_string();
            let result = (|| {
                let update = fs::read(&file).map_err(|e| e.to_string())?;
                let installed = fs::read(game.join(PACK_FILE)).map_err(|e| format!("не найдена установленная история: {e}"))?;
                let (merged, manifest) = pack::merge_update(installed, update)?;
                saves(&manifest.id).backup(&format!("до выпуска {}", manifest.story_version))?;
                write_atomic(&game.join(PACK_FILE), &merged).map_err(|e| e.to_string())?;
                let done = game.join("game_data/applied");
                fs::create_dir_all(&done).map_err(|e| e.to_string())?;
                fs::rename(&file, done.join(&name)).map_err(|e| e.to_string())?;
                Ok(manifest)
            })();
            match result {
                Ok(m) => UpdateOutcome::Applied(m),
                Err(reason) => {
                    // Отвергнутый файл помечается, чтобы игра не спотыкалась о него при каждом запуске.
                    let _ = fs::rename(&file, file.with_extension("vnpak.rejected"));
                    UpdateOutcome::Rejected { file: name, reason }
                }
            }
        })
        .collect()
}

const WEB_FILES: [(&str, &str); 5] = [
    ("index.html", include_str!("../../../player-web/index.html")),
    ("style.css", include_str!("../../../player-web/style.css")),
    ("runtime.js", include_str!("../../../player-web/runtime.js")),
    ("player.js", include_str!("../../../player-web/player.js")),
    ("icon.svg", r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><rect width="512" height="512" rx="96" fill="#191a36"/><rect x="72" y="300" width="368" height="130" rx="24" fill="#6c63ff"/><circle cx="256" cy="180" r="92" fill="#e8c26a"/></svg>"##),
];
const SERVICE_WORKER: &str = include_str!("../../../player-web/sw.js");

/// Собрать веб-версию для телефона: папку `builds/<имя>-web/` и готовый к загрузке
/// на хостинг архив `builds/<имя>-web.zip`. Возвращает папку.
pub fn export_web(project: &Path, story: &Story) -> Result<PathBuf, String> {
    let name = safe_name(&story.meta.name);
    let out = project.join("builds").join(format!("{name}-web"));
    let e = io("Не удалось собрать веб-версию");
    if out.exists() {
        fs::remove_dir_all(&out).map_err(&e)?;
    }
    let mut files: Vec<(String, Vec<u8>)> = WEB_FILES.iter().map(|(n, c)| (n.to_string(), c.as_bytes().to_vec())).collect();
    files[0].1 = String::from_utf8_lossy(&files[0].1).replace("<title>Визуальная новелла</title>", &format!("<title>{}</title>", html_escape(&story.meta.name))).into_bytes();
    files.push(("story.json".into(), serde_json::to_vec(story).map_err(|x| x.to_string())?));
    let manifest = serde_json::json!({
        "name": story.meta.name, "short_name": story.meta.name, "description": story.meta.description,
        "start_url": "./index.html", "scope": "./", "display": "fullscreen", "orientation": "landscape",
        "background_color": "#0d0d14", "theme_color": "#0d0d14",
        "icons": [{ "src": "icon.svg", "sizes": "any", "type": "image/svg+xml", "purpose": "any maskable" }]
    });
    files.push(("manifest.webmanifest".into(), serde_json::to_vec_pretty(&manifest).map_err(|x| x.to_string())?));
    for asset in story.assets_used() {
        let bytes = fs::read(project.join(&asset)).map_err(|err| format!("Ресурс «{asset}» не прочитан: {err}"))?;
        files.push((asset, bytes));
    }
    let list: Vec<String> = std::iter::once("./".to_string()).chain(files.iter().map(|(n, _)| n.split('/').map(url_encode).collect::<Vec<_>>().join("/"))).collect();
    let sw = SERVICE_WORKER.replace("__CACHE__", &format!("vn-{}-{}-{}", safe_name(&story.meta.id), story.meta.story_version, now())).replace("__FILES__", &serde_json::to_string(&list).map_err(|x| x.to_string())?);
    files.push(("sw.js".into(), sw.into_bytes()));

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (rel, bytes) in &files {
        write_atomic(&out.join(rel), bytes).map_err(&e)?;
        zip.start_file(rel.as_str(), zip::write::SimpleFileOptions::default()).map_err(|x| x.to_string())?;
        zip.write_all(bytes).map_err(&e)?;
    }
    let archive = zip.finish().map_err(|x| x.to_string())?.into_inner();
    write_atomic(&project.join("builds").join(format!("{name}-web.zip")), &archive).map_err(&e)?;
    Ok(out)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Кодирование части пути так же, как это делает `encodeURI` в плеере.
fn url_encode(part: &str) -> String {
    part.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~!*'()".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::Pack;
    use vn_core::model::*;
    use vn_core::runtime::Runtime;

    fn project() -> (tempfile::TempDir, Story) {
        let tmp = tempfile::tempdir().unwrap();
        let mut story = crate::project::create(tmp.path(), "Моя <новелла>").unwrap();
        fs::write(tmp.path().join("assets/backgrounds/Комната 1.png"), b"PNG").unwrap();
        let say = |t: &str| Event::new(EventKind::Say { speaker: None, style: TextStyle::Narration, text: t.into(), expression: None, speed: None, sound: None, look: TextLook::default() });
        story.scenes[0].events = vec![Event::new(EventKind::Background { image: "assets/backgrounds/Комната 1.png".into(), fade: 0.0 }), say("Первая"), say("Вторая"), Event::new(EventKind::End { title: "Конец".into() })];
        (tmp, story)
    }

    fn fake_player(dir: &Path) -> PathBuf {
        let p = dir.join("vn-player.exe");
        fs::write(&p, b"MZ").unwrap();
        p
    }

    #[test]
    fn desktop_build_is_a_self_contained_folder() {
        let (tmp, story) = project();
        let out = export_desktop(tmp.path(), &story, &fake_player(tmp.path())).unwrap();
        assert_eq!(out, tmp.path().join("builds/Моя__новелла_"));
        assert_eq!(fs::read(out.join("Моя__новелла_.exe")).unwrap(), b"MZ");
        let pack = Pack::open(fs::read(out.join(PACK_FILE)).unwrap()).unwrap();
        assert_eq!(pack.story, story);
        assert!(pack.asset("assets/backgrounds/Комната 1.png").is_some());
        assert!(fs::read_to_string(out.join("README.txt")).unwrap().contains("Как установить обновление"));
        assert!(out.join("config/game.json").exists());
        assert!(export_desktop(tmp.path(), &story, Path::new("/nope/player.exe")).unwrap_err().contains("Не найден плеер"));
    }

    #[test]
    fn released_update_installs_and_the_player_continues_from_the_save() {
        let (tmp, mut story) = project();
        let game = export_desktop(tmp.path(), &story, &fake_player(tmp.path())).unwrap();
        let saves_dir = tmp.path().join("userdata");
        let saves = |_: &str| SaveStore::at(saves_dir.clone());

        // Игрок версии 1 дошёл до второй реплики и сохранился.
        let mut rt = Runtime::new(Pack::open(fs::read(game.join(PACK_FILE)).unwrap()).unwrap().story);
        rt.advance();
        saves("").write(crate::saves::QUICK, &rt.save(100)).unwrap();

        // Автор дописал продолжение и выпустил обновление только сценария.
        story.meta.story_version = 2;
        story.meta.version = "1.1".into();
        let mut next = Scene::new("Глава 2");
        next.events = vec![Event::new(EventKind::Say { speaker: None, style: TextStyle::Narration, text: "Продолжение".into(), expression: None, speed: None, sound: None, look: TextLook::default() }), Event::new(EventKind::End { title: "Конец главы 2".into() })];
        story.scenes[0].events.pop();
        story.scenes[0].events.push(Event::new(EventKind::Goto { scene: next.id.clone() }));
        story.scenes.push(next);
        let update = export_update(tmp.path(), &story, false).unwrap();
        assert!(tmp.path().join("changelog/release-2.txt").exists());
        fs::copy(&update, game.join("update.vnpak")).unwrap();
        fs::write(game.join("чужое.vnpak"), b"junk").unwrap();

        let outcomes = apply_updates(&game, &saves);
        assert!(matches!(&outcomes[0], UpdateOutcome::Applied(m) if m.story_version == 2));
        assert!(matches!(&outcomes[1], UpdateOutcome::Rejected { file, .. } if file == "чужое.vnpak"));
        assert!(!game.join("update.vnpak").exists() && game.join("game_data/applied/update.vnpak").exists());
        assert!(fs::read_dir(saves_dir.join("backups")).unwrap().count() == 1, "сохранения скопированы перед обновлением");

        let pack = Pack::open(fs::read(game.join(PACK_FILE)).unwrap()).unwrap();
        assert!(pack.asset("assets/backgrounds/Комната 1.png").is_some(), "ресурсы на месте");
        let save = saves("").read(crate::saves::QUICK).unwrap().unwrap();
        let (mut rt, _) = Runtime::restore(pack.story, save).unwrap();
        assert_eq!(rt.state.backlog.last().unwrap().text, "Вторая", "игрок там же, где сохранился");
        rt.advance();
        assert_eq!(rt.state.backlog.last().unwrap().text, "Продолжение", "и попадает в новую главу");

        assert!(apply_updates(&game, &saves).is_empty(), "повторный запуск ничего не делает");
    }

    #[test]
    fn web_build_has_player_story_assets_and_offline_list() {
        let (tmp, story) = project();
        let out = export_web(tmp.path(), &story).unwrap();
        for f in ["index.html", "style.css", "runtime.js", "player.js", "sw.js", "manifest.webmanifest", "icon.svg", "story.json", "assets/backgrounds/Комната 1.png"] {
            assert!(out.join(f).is_file(), "{f}");
        }
        assert_eq!(serde_json::from_slice::<Story>(&fs::read(out.join("story.json")).unwrap()).unwrap(), story);
        assert!(fs::read_to_string(out.join("index.html")).unwrap().contains("<title>Моя &lt;новелла&gt;</title>"));
        let sw = fs::read_to_string(out.join("sw.js")).unwrap();
        assert!(sw.contains("assets/backgrounds/%D0%9A%D0%BE%D0%BC%D0%BD%D0%B0%D1%82%D0%B0%201.png") && !sw.contains("__FILES__") && !sw.contains("__CACHE__"));
        let mut zip = zip::ZipArchive::new(fs::File::open(tmp.path().join("builds/Моя__новелла_-web.zip")).unwrap()).unwrap();
        assert!(zip.by_name("index.html").is_ok() && zip.by_name("story.json").is_ok());
        export_web(tmp.path(), &story).unwrap();
    }
}
