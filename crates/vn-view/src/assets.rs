//! Источники ресурсов и кэш текстур с фоновой загрузкой.
use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Откуда игра берёт файлы: папка проекта (редактор) или пакет `.vnpak` (игра).
pub trait Assets: Send + Sync {
    fn read(&self, path: &str) -> Option<Vec<u8>>;
}

pub struct DirAssets(pub PathBuf);

impl Assets for DirAssets {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(path)).ok()
    }
}

pub struct PackAssets(pub vn_store::pack::Pack);

impl Assets for PackAssets {
    fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.0.asset(path)
    }
}

/// Крупные изображения уменьшаются до этого размера по большей стороне.
const MAX_SIDE: u32 = 2560;
/// Текстура, не нужная столько секунд, выгружается из памяти.
const KEEP_SECONDS: f64 = 90.0;

fn decode(bytes: &[u8]) -> Option<ColorImage> {
    let mut img = image::load_from_memory(bytes).ok()?;
    if img.width().max(img.height()) > MAX_SIDE {
        img = img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle);
    }
    let rgba = img.to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

enum Slot {
    Loading,
    Missing,
    Ready(TextureHandle, f64),
}

/// Кэш текстур. Файлы читаются и декодируются в фоновом потоке, окно при этом не замирает.
pub struct Textures {
    assets: Arc<dyn Assets>,
    slots: HashMap<String, Slot>,
    tx: Sender<(String, Option<ColorImage>)>,
    rx: Receiver<(String, Option<ColorImage>)>,
}

impl Textures {
    pub fn new(assets: Arc<dyn Assets>) -> Self {
        let (tx, rx) = channel();
        Self { assets, slots: HashMap::new(), tx, rx }
    }

    /// Текстура по пути ресурса; `None`, пока она грузится или если файла нет.
    pub fn get(&mut self, ctx: &Context, path: &str) -> Option<TextureHandle> {
        while let Ok((name, image)) = self.rx.try_recv() {
            let slot = match image {
                Some(img) => Slot::Ready(ctx.load_texture(&name, img, TextureOptions::LINEAR), 0.0),
                None => Slot::Missing,
            };
            self.slots.insert(name, slot);
        }
        if path.is_empty() {
            return None;
        }
        let now = ctx.input(|i| i.time);
        match self.slots.get_mut(path) {
            Some(Slot::Ready(tex, used)) => {
                *used = now;
                Some(tex.clone())
            }
            Some(_) => None,
            None => {
                self.slots.insert(path.to_string(), Slot::Loading);
                let (assets, tx, ctx, name) = (self.assets.clone(), self.tx.clone(), ctx.clone(), path.to_string());
                std::thread::spawn(move || {
                    let image = assets.read(&name).and_then(|b| decode(&b));
                    let _ = tx.send((name, image));
                    ctx.request_repaint();
                });
                None
            }
        }
    }

    /// Все ли перечисленные изображения уже загружены (или отсутствуют).
    pub fn settled(&self, paths: &[&str]) -> bool {
        paths.iter().all(|p| p.is_empty() || matches!(self.slots.get(*p), Some(Slot::Ready(..) | Slot::Missing)))
    }

    /// Выгрузить давно не использованные текстуры.
    pub fn collect_garbage(&mut self, now: f64) {
        self.slots.retain(|_, s| !matches!(s, Slot::Ready(_, used) if now - *used > KEEP_SECONDS));
    }

    /// Забыть файл (его заменили на диске) или все файлы сразу.
    pub fn forget(&mut self, path: Option<&str>) {
        match path {
            Some(p) => drop(self.slots.remove(p)),
            None => self.slots.clear(),
        }
    }
}
