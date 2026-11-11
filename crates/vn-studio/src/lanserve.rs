//! Маленький веб-сервер для проверки веб-версии на телефоне в той же сети Wi-Fi.
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::path::{Component, Path, PathBuf};

pub struct Server {
    pub port: u16,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "webmanifest" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

fn decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%').then(|| path.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())).flatten();
        match hex {
            Some(b) => {
                out.push(b);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Файл внутри `root` для пути запроса; выход за пределы папки запрещён.
pub fn resolve(root: &Path, target: &str) -> Option<PathBuf> {
    let path = decode(target.split(['?', '#']).next().unwrap_or(""));
    let rel = Path::new(path.trim_start_matches('/'));
    if rel.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    let full = if rel.as_os_str().is_empty() { root.join("index.html") } else { root.join(rel) };
    full.is_file().then_some(full)
}

fn handle(stream: TcpStream, root: &Path) -> std::io::Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    let target = line.split_whitespace().nth(1).unwrap_or("/");
    let mut out = &stream;
    match resolve(root, target) {
        Some(file) => {
            let body = std::fs::read(&file)?;
            write!(out, "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n", mime(&file), body.len())?;
            out.write_all(&body)
        }
        None => out.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    }
}

impl Server {
    /// Начать раздавать папку. Порт выбирает система.
    pub fn start(root: PathBuf) -> Result<Server, String> {
        let listener = TcpListener::bind("0.0.0.0:0").map_err(|e| format!("Не удалось запустить сервер: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                if flag.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }
                let root = root.clone();
                std::thread::spawn(move || drop(handle(stream, &root)));
            }
        });
        Ok(Server { port, stop })
    }

    /// Адрес, который нужно открыть на телефоне.
    pub fn url(&self) -> String {
        let ip = UdpSocket::bind("0.0.0.0:0").and_then(|s| s.connect("192.0.2.1:9").and_then(|_| s.local_addr())).map(|a| a.ip().to_string()).unwrap_or_else(|_| "127.0.0.1".into());
        format!("http://{ip}:{}/", self.port)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn get(port: u16, path: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        out
    }

    #[test]
    fn serves_the_game_folder_and_nothing_outside_it() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("web");
        std::fs::create_dir_all(root.join("assets")).unwrap();
        std::fs::write(root.join("index.html"), "<p>игра</p>").unwrap();
        std::fs::write(root.join("assets/мой фон.png"), "PNG").unwrap();
        std::fs::write(tmp.path().join("secret.txt"), "secret").unwrap();
        let server = Server::start(root).unwrap();
        let index = get(server.port, "/");
        assert!(index.starts_with("HTTP/1.1 200") && index.contains("text/html") && index.ends_with("<p>игра</p>"), "{index}");
        let image = get(server.port, "/assets/%D0%BC%D0%BE%D0%B9%20%D1%84%D0%BE%D0%BD.png?v=1");
        assert!(image.contains("image/png") && image.ends_with("PNG"), "{image}");
        assert!(get(server.port, "/../secret.txt").starts_with("HTTP/1.1 404"));
        assert!(get(server.port, "/%2e%2e/secret.txt").starts_with("HTTP/1.1 404"));
        assert!(get(server.port, "/nope.js").starts_with("HTTP/1.1 404"));
        assert!(server.url().starts_with("http://") && server.url().ends_with(&format!(":{}/", server.port)));
    }
}
