//! Сетевые части редактора против настоящего HTTP-сервера на этом же компьютере:
//! запрос действительно уходит по сети, ответ действительно разбирается.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use vn_studio::ai::{self, AiSettings};
use vn_studio::update::{self, Check};

type Seen = Arc<Mutex<Vec<(String, String, String)>>>;

/// Сервер, отвечающий по таблице «путь → (код, тело)». Запоминает, что у него спросили:
/// строку запроса, заголовки и тело.
fn serve(routes: Vec<(&'static str, u16, Vec<u8>)>) -> (String, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen: Seen = Default::default();
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(&stream);
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            let (mut headers, mut length) = (String::new(), 0usize);
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                headers.push_str(&line);
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_string();
            log.lock().unwrap().push((request.trim().to_string(), headers, String::from_utf8_lossy(&body).into_owned()));
            let (status, reply) = routes.iter().find(|(p, ..)| *p == path).map(|(_, s, b)| (*s, b.clone())).unwrap_or((404, b"{\"error\":\"not found\"}".to_vec()));
            let mut out = &stream;
            write!(out, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", reply.len()).unwrap();
            out.write_all(&reply).unwrap();
        }
    });
    (base, seen)
}

fn local_ai(base: &str, preset: usize, key: &str) -> AiSettings {
    let mut s = AiSettings::default();
    s.use_preset(preset);
    s.base_url = format!("{base}/v1");
    s.api_key = key.into();
    s.model = "test-model".into();
    s
}

#[test]
fn local_ai_server_is_asked_and_its_story_becomes_a_draft() {
    let story = r#"{"scenes":[{"key":"a","name":"Причал","lines":[{"text":"Туман."}],"ending":"Конец"}]}"#;
    let reply = serde_json::json!({"choices": [{"message": {"role": "assistant", "content": story}}]});
    let models = serde_json::json!({"object": "list", "data": [{"id": "qwen"}, {"id": "llama"}]});
    let (base, seen) = serve(vec![("/v1/chat/completions", 200, reply.to_string().into_bytes()), ("/v1/models", 200, models.to_string().into_bytes())]);
    let settings = local_ai(&base, 0, "");
    assert_eq!(ai::list_models(&settings).unwrap(), vec!["llama", "qwen"]);
    let text = ai::ask(&settings, "system rules", "история про маяк").unwrap();
    let draft = vn_core::draft::parse_draft(&text).unwrap();
    assert_eq!(draft.scenes[0].name, "Причал");
    let seen = seen.lock().unwrap();
    let (request, headers, body) = &seen[1];
    assert_eq!(request, "POST /v1/chat/completions HTTP/1.1");
    assert!(!headers.to_lowercase().contains("authorization"), "локальному ИИ ключ не отправляется");
    let sent: serde_json::Value = serde_json::from_str(body).unwrap();
    assert_eq!((sent["model"].as_str(), sent["messages"][1]["content"].as_str()), (Some("test-model"), Some("история про маяк")));
}

#[test]
fn cloud_ai_gets_the_key_and_its_refusal_is_explained() {
    let reply = serde_json::json!({"content": [{"type": "text", "text": "{\"scenes\":[]}"}]});
    let (base, seen) = serve(vec![("/v1/messages", 200, reply.to_string().into_bytes())]);
    assert_eq!(ai::ask(&local_ai(&base, 4, "secret-key"), "s", "u").unwrap(), "{\"scenes\":[]}");
    let headers = seen.lock().unwrap()[0].1.to_lowercase();
    assert!(headers.contains("x-api-key: secret-key") && headers.contains("anthropic-version:"), "{headers}");

    let (base, _) = serve(vec![("/v1/chat/completions", 401, br#"{"error":{"message":"Incorrect API key"}}"#.to_vec())]);
    let e = ai::ask(&local_ai(&base, 2, "bad"), "s", "u").unwrap_err();
    assert!(e.contains("401") && e.contains("Incorrect API key") && e.contains("ключ"), "{e}");
}

#[test]
fn ai_that_is_not_running_gives_a_helpful_message() {
    let free = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
    let e = ai::list_models(&local_ai(&format!("http://{free}"), 0, "")).unwrap_err();
    assert!(e.contains("Не удалось связаться") && e.contains("запущена"), "{e}");
}

fn zip_with(name: &str, text: &str) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(vec![]));
    w.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
    w.write_all(text.as_bytes()).unwrap();
    w.finish().unwrap().into_inner()
}

#[test]
fn update_button_finds_a_newer_release_downloads_and_installs_it() {
    let latest = format!("/repos/{}/releases/latest", update::REPO);
    let latest: &'static str = Box::leak(latest.into_boxed_str());
    // Адрес архива известен только после запуска сервера, поэтому серверов два: API и файлы.
    let (files, _) = serve(vec![("/download.zip", 200, zip_with("vn-studio.exe", "version 99"))]);
    let release = serde_json::json!({"tag_name": "v99.0.0", "body": "Новое", "html_url": "https://example.test", "assets": [{"name": format!("vn-studio-v99.0.0-{}", update::ASSET_SUFFIX), "browser_download_url": format!("{files}/download.zip")}]});
    let (api, seen) = serve(vec![(latest, 200, release.to_string().into_bytes())]);
    let Check::Available(found) = update::check_at(&api).unwrap() else { panic!("выпуск 99 новее установленного") };
    assert!(seen.lock().unwrap()[0].1.to_lowercase().contains("user-agent: vn-studio"), "GitHub отклоняет запросы без User-Agent");
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("vn-studio.exe"), "version 0").unwrap();
    update::install_to(&found, tmp.path()).unwrap();
    assert_eq!(std::fs::read_to_string(tmp.path().join("vn-studio.exe")).unwrap(), "version 99");
    assert_eq!(std::fs::read_to_string(tmp.path().join("vn-studio.exe.old")).unwrap(), "version 0");
}

#[test]
fn update_button_handles_no_releases_same_version_and_broken_download() {
    let latest: &'static str = Box::leak(format!("/repos/{}/releases/latest", update::REPO).into_boxed_str());
    let (api, _) = serve(vec![]);
    assert!(update::check_at(&api).unwrap_err().contains("нет опубликованных выпусков"));
    let same = serde_json::json!({"tag_name": format!("v{}", update::CURRENT), "assets": []});
    let (api, _) = serve(vec![(latest, 200, same.to_string().into_bytes())]);
    assert_eq!(update::check_at(&api).unwrap(), Check::UpToDate);
    let (files, _) = serve(vec![("/bad.zip", 200, b"oops".to_vec())]);
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("vn-studio.exe"), "version 0").unwrap();
    let broken = update::Release { tag: "v9".into(), notes: String::new(), page: String::new(), asset: Some(format!("{files}/bad.zip")) };
    assert!(update::install_to(&broken, tmp.path()).unwrap_err().contains("повреждён"));
    let missing = update::Release { asset: Some(format!("{files}/none.zip")), ..broken };
    assert!(update::install_to(&missing, tmp.path()).unwrap_err().contains("404"));
    assert_eq!(std::fs::read_to_string(tmp.path().join("vn-studio.exe")).unwrap(), "version 0", "неудачное обновление ничего не портит");
}
