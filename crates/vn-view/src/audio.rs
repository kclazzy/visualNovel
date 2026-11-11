//! Музыка и звуки. Устройство открывается при первом звуке; без звуковой карты игра молчит, но работает.
#[cfg(feature = "audio")]
mod backend {
    use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player};
    use std::io::Cursor;

    #[derive(Default)]
    pub struct Device(Option<MixerDeviceSink>, bool);

    pub struct Voice(Player);

    impl Device {
        fn sink(&mut self) -> Option<&MixerDeviceSink> {
            if self.0.is_none() && !self.1 {
                self.1 = true;
                self.0 = DeviceSinkBuilder::open_default_sink().ok().map(|mut s| {
                    s.log_on_drop(false);
                    s
                });
            }
            self.0.as_ref()
        }

        pub fn play(&mut self, bytes: Vec<u8>, looped: bool, volume: f32) -> Option<Voice> {
            let player = Player::connect_new(self.sink()?.mixer());
            player.set_volume(volume);
            if looped {
                player.append(Decoder::new_looped(Cursor::new(bytes)).ok()?);
            } else {
                player.append(Decoder::new(Cursor::new(bytes)).ok()?);
            }
            Some(Voice(player))
        }
    }

    impl Voice {
        pub fn set_volume(&self, v: f32) {
            self.0.set_volume(v);
        }
        pub fn finished(&self) -> bool {
            self.0.empty()
        }
        pub fn pause(&self, on: bool) {
            if on { self.0.pause() } else { self.0.play() }
        }
    }
}

#[cfg(not(feature = "audio"))]
mod backend {
    #[derive(Default)]
    pub struct Device;
    pub struct Voice;
    impl Device {
        pub fn play(&mut self, _bytes: Vec<u8>, _looped: bool, _volume: f32) -> Option<Voice> {
            None
        }
    }
    impl Voice {
        pub fn set_volume(&self, _v: f32) {}
        pub fn finished(&self) -> bool {
            true
        }
        pub fn pause(&self, _on: bool) {}
    }
}

use backend::{Device, Voice};

struct Track {
    name: String,
    voice: Option<Voice>,
    /// Текущая и целевая громкость дорожки (без учёта настройки игрока).
    level: f32,
    target: f32,
    /// Скорость изменения громкости в секунду.
    rate: f32,
}

#[derive(Default)]
pub struct Audio {
    device: Device,
    music: Option<Track>,
    fading: Vec<Track>,
    sounds: Vec<Voice>,
    master: f32,
}

/// Громкость после шага плавного изменения.
pub fn ramp(level: f32, target: f32, rate: f32, dt: f32) -> f32 {
    if rate <= 0.0 || (target - level).abs() <= rate * dt {
        target
    } else {
        level + (target - level).signum() * rate * dt
    }
}

impl Audio {
    /// Привести музыку к желаемой: `(файл, громкость, зациклить)` или тишина.
    pub fn set_music(&mut self, want: Option<(&str, f32, bool)>, fade: f32, load: &dyn Fn(&str) -> Option<Vec<u8>>) {
        let rate = if fade > 0.0 { 1.0 / fade } else { 0.0 };
        let same = matches!((&self.music, want), (Some(t), Some((name, ..))) if t.name == name);
        if !same {
            if let Some(mut old) = self.music.take() {
                old.target = 0.0;
                old.rate = rate;
                self.fading.push(old);
            }
            if let Some((name, volume, looped)) = want {
                let voice = load(name).and_then(|bytes| self.device.play(bytes, looped, 0.0));
                self.music = Some(Track { name: name.to_string(), voice, level: if fade > 0.0 { 0.0 } else { volume }, target: volume, rate });
            }
        } else if let (Some(t), Some((_, volume, _))) = (&mut self.music, want) {
            if t.target != volume {
                t.target = volume;
                t.rate = rate;
            }
        }
    }

    pub fn play_sound(&mut self, bytes: Vec<u8>, volume: f32) {
        self.sounds.retain(|s| !s.finished());
        if let Some(v) = self.device.play(bytes, false, volume) {
            self.sounds.push(v);
        }
    }

    /// Шаг плавных переходов. `master` — громкость музыки из настроек игрока.
    /// Возвращает `true`, пока громкость ещё меняется (нужна перерисовка).
    pub fn update(&mut self, dt: f32, master: f32) -> bool {
        self.master = master;
        let mut busy = false;
        for t in self.music.iter_mut().chain(self.fading.iter_mut()) {
            t.level = ramp(t.level, t.target, t.rate, dt);
            busy |= t.level != t.target;
            if let Some(v) = &t.voice {
                v.set_volume(t.level * master);
            }
        }
        self.fading.retain(|t| t.level > 0.0);
        busy
    }

    pub fn pause(&mut self, on: bool) {
        for v in self.music.iter().chain(&self.fading).filter_map(|t| t.voice.as_ref()) {
            v.pause(on);
        }
    }

    pub fn stop_all(&mut self) {
        self.music = None;
        self.fading.clear();
        self.sounds.clear();
    }

    /// Имя играющей дорожки — для режима отладки.
    pub fn current_track(&self) -> Option<&str> {
        self.music.as_ref().map(|t| t.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_ramps_linearly_and_lands_exactly() {
        assert_eq!(ramp(0.0, 1.0, 0.0, 0.1), 1.0, "без плавности — сразу");
        assert!((ramp(0.0, 1.0, 2.0, 0.1) - 0.2).abs() < 1e-6);
        assert!((ramp(1.0, 0.0, 2.0, 0.1) - 0.8).abs() < 1e-6);
        assert_eq!(ramp(0.95, 1.0, 2.0, 0.1), 1.0);
    }

    #[test]
    fn music_switch_fades_old_track_out_and_new_in() {
        let mut a = Audio::default();
        let none = |_: &str| None;
        a.set_music(Some(("a.ogg", 0.8, true)), 1.0, &none);
        assert!(a.update(0.5, 1.0));
        assert_eq!(a.current_track(), Some("a.ogg"));
        assert!((a.music.as_ref().unwrap().level - 0.5).abs() < 1e-6);
        a.set_music(Some(("b.ogg", 1.0, true)), 0.5, &none);
        assert_eq!((a.current_track(), a.fading.len()), (Some("b.ogg"), 1));
        a.update(1.0, 1.0);
        assert!(a.fading.is_empty(), "старая дорожка затихла и убрана");
        assert!(!a.update(1.0, 1.0));
        a.set_music(Some(("b.ogg", 0.2, true)), 0.0, &none);
        a.update(0.01, 1.0);
        assert_eq!(a.music.as_ref().unwrap().level, 0.2, "смена громкости той же дорожки");
        a.set_music(None, 0.0, &none);
        a.update(0.01, 1.0);
        assert!(a.current_track().is_none() && a.fading.is_empty());
    }
}
