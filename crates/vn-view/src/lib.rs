//! Отрисовка игры на egui. Один и тот же код показывает игру в плеере
//! и предпросмотр в редакторе, поэтому предпросмотр совпадает с готовой игрой.
pub mod assets;
pub mod audio;
pub mod draw;
pub mod game;

pub use assets::{Assets, DirAssets, PackAssets, Textures};
pub use game::GameView;

/// Подключить запасной шрифт со стрелками и значками, которых нет в шрифте egui по умолчанию.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("DejaVuSans".into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../fonts/DejaVuSans.ttf"))));
    for family in fonts.families.values_mut() {
        family.push("DejaVuSans".into());
    }
    ctx.set_fonts(fonts);
}
