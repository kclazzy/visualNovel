//! Рисование сцены: фон, спрайты, окно диалога. Геометрия вынесена в чистые функции.
use crate::assets::Textures;
use egui::text::{LayoutJob, TextFormat};
use egui::{pos2, vec2, Color32, Context, CornerRadius, FontId, Painter, Rect, Stroke, StrokeKind, Vec2};
use vn_core::model::{DialogPos, TextSettings, TextStyle};
use vn_core::view::{Span, Speaker};

pub const ACCENT: Color32 = Color32::from_rgb(108, 99, 255);
pub const GOLD: Color32 = Color32::from_rgb(255, 214, 107);
pub const PANEL: Color32 = Color32::from_rgba_premultiplied(11, 14, 31, 200);

/// Вписать экран игры с нужными пропорциями в доступную область (с полями по краям).
pub fn fit(avail: Rect, resolution: [u32; 2]) -> Rect {
    let aspect = resolution[0].max(1) as f32 / resolution[1].max(1) as f32;
    let size = if avail.width() / avail.height().max(1.0) > aspect { vec2(avail.height() * aspect, avail.height()) } else { vec2(avail.width(), avail.width() / aspect) };
    Rect::from_center_size(avail.center(), size)
}

/// Прямоугольник спрайта: `x` — центр, `y` — низ, `height` — доля высоты экрана.
pub fn sprite_rect(stage: Rect, x: f32, y: f32, height: f32, aspect: f32) -> Rect {
    let h = height * stage.height();
    let w = h * aspect;
    let bottom = stage.top() + y * stage.height();
    let cx = stage.left() + x * stage.width();
    Rect::from_min_max(pos2(cx - w / 2.0, bottom - h), pos2(cx + w / 2.0, bottom))
}

/// Обратное к `sprite_rect`: позиция персонажа по точке под курсором (для перетаскивания).
pub fn stage_point(stage: Rect, p: egui::Pos2) -> (f32, f32) {
    ((p.x - stage.left()) / stage.width(), (p.y - stage.top()) / stage.height())
}

/// Часть изображения, которая заполнит прямоугольник целиком без искажения пропорций.
pub fn cover_uv(image: Vec2, rect: Rect) -> Rect {
    let (ia, ra) = (image.x / image.y.max(1.0), rect.width() / rect.height().max(1.0));
    let (w, h) = if ia > ra { (ra / ia, 1.0) } else { (1.0, ia / ra) };
    Rect::from_center_size(pos2(0.5, 0.5), vec2(w, h))
}

const FULL_UV: Rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));

fn tint(alpha: f32) -> Color32 {
    Color32::WHITE.gamma_multiply(alpha.clamp(0.0, 1.0))
}

pub fn background(painter: &Painter, ctx: &Context, textures: &mut Textures, stage: Rect, image: &str, alpha: f32) {
    if let Some(tex) = textures.get(ctx, image) {
        painter.image(tex.id(), stage, cover_uv(tex.size_vec2(), stage), tint(alpha));
    }
}

/// Нарисовать спрайт из слоёв. Возвращает занятый прямоугольник, когда изображение загружено.
/// `place` — центр по X, низ по Y и высота в долях экрана.
pub fn sprite(painter: &Painter, ctx: &Context, textures: &mut Textures, stage: Rect, layers: &[String], place: [f32; 3], alpha: f32) -> Option<Rect> {
    let loaded: Vec<_> = layers.iter().filter_map(|l| textures.get(ctx, l)).collect();
    let size = loaded.first()?.size_vec2();
    let rect = sprite_rect(stage, place[0], place[1], place[2], size.x / size.y.max(1.0));
    for tex in &loaded {
        painter.image(tex.id(), rect, FULL_UV, tint(alpha));
    }
    Some(rect)
}

/// Заглушка на месте персонажа без изображения: редактор показывает, где он стоит.
pub fn placeholder(painter: &Painter, stage: Rect, name: &str, color: [u8; 3], x: f32, y: f32, height: f32) -> Rect {
    let rect = sprite_rect(stage, x, y, height, 0.42);
    let c = Color32::from_rgb(color[0], color[1], color[2]);
    painter.rect(rect, CornerRadius::same(8), c.gamma_multiply(0.25), Stroke::new(2.0, c), StrokeKind::Inside);
    painter.text(rect.center(), egui::Align2::CENTER_CENTER, name, FontId::proportional(stage.height() * 0.035), Color32::WHITE);
    rect
}

pub fn char_count(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.text.chars().count()).sum()
}

/// Текст реплики: первые `shown` символов видимы, остальные прозрачны (вёрстка не прыгает).
pub fn text_job(spans: &[Span], shown: usize, size: f32, color: Color32, style: TextStyle, width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let mut left = shown;
    for s in spans {
        let visible_color = match (s.bold, style) {
            (true, _) => GOLD,
            (false, TextStyle::Thought) => Color32::from_rgb(201, 212, 255),
            _ => color,
        };
        let split = s.text.char_indices().nth(left).map(|(i, _)| i).unwrap_or(s.text.len());
        left = left.saturating_sub(s.text.chars().count());
        let font = |c: Color32| TextFormat { font_id: FontId::proportional(size), color: c, italics: style == TextStyle::Thought, ..Default::default() };
        if split > 0 {
            job.append(&s.text[..split], 0.0, font(visible_color));
        }
        if split < s.text.len() {
            job.append(&s.text[split..], 0.0, font(Color32::TRANSPARENT));
        }
    }
    job
}

/// Область окна диалога на экране.
pub fn dialog_rect(stage: Rect, pos: DialogPos) -> Rect {
    let (w, h) = (stage.width() * 0.94, stage.height() * 0.27);
    let top = match pos {
        DialogPos::Top => stage.top() + stage.height() * 0.1,
        DialogPos::Center => stage.center().y - h / 2.0,
        DialogPos::Bottom => stage.bottom() - h - stage.height() * 0.03,
    };
    Rect::from_min_size(pos2(stage.center().x - w / 2.0, top), vec2(w, h))
}

/// Реплика для окна диалога: кто говорит, как и сколько символов уже показано.
pub struct Line<'a> {
    pub speaker: Option<&'a Speaker>,
    pub style: TextStyle,
    pub spans: &'a [Span],
    pub shown: usize,
}

/// Окно диалога с именем говорящего и текстом.
pub fn dialog(painter: &Painter, stage: Rect, pos: DialogPos, line: &Line<'_>, text: &TextSettings) -> Rect {
    let Line { speaker, style, spans, shown } = *line;
    let rect = dialog_rect(stage, pos);
    let unit = stage.height();
    let size = text.size * unit;
    let border = if style == TextStyle::System { ACCENT } else { Color32::from_white_alpha(80) };
    painter.rect(rect, CornerRadius::same((unit * 0.02) as u8), if style == TextStyle::System { Color32::from_black_alpha(215) } else { PANEL }, Stroke::new(1.5, border), StrokeKind::Inside);
    let pad = unit * 0.035;
    let color = Color32::from_rgb(text.color[0], text.color[1], text.color[2]);
    let galley = painter.layout_job(text_job(spans, shown, size, color, style, rect.width() - pad * 2.0));
    let at = if style == TextStyle::System { pos2(rect.center().x - galley.size().x / 2.0, rect.top() + pad) } else { rect.min + vec2(pad, pad) };
    painter.galley(at, galley, color);
    if let Some(sp) = speaker {
        let name = painter.layout_no_wrap(sp.name.clone(), FontId::proportional(size), Color32::from_rgb(sp.color[0], sp.color[1], sp.color[2]));
        let plate = Rect::from_min_size(rect.min + vec2(pad, -name.size().y * 0.75), name.size() + vec2(pad, unit * 0.012));
        painter.rect(plate, CornerRadius::same((unit * 0.012) as u8), Color32::from_rgb(11, 14, 31), Stroke::new(1.0, Color32::from_white_alpha(80)), StrokeKind::Inside);
        painter.galley(plate.min + vec2(pad / 2.0, unit * 0.006), name, Color32::WHITE);
    }
    rect
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage() -> Rect {
        Rect::from_min_size(pos2(100.0, 50.0), vec2(1600.0, 900.0))
    }

    #[test]
    fn screen_is_letterboxed_into_any_window() {
        let wide = fit(Rect::from_min_size(pos2(0.0, 0.0), vec2(2000.0, 720.0)), [1280, 720]);
        assert_eq!((wide.width(), wide.height(), wide.left()), (1280.0, 720.0, 360.0));
        let tall = fit(Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 1000.0)), [1280, 720]);
        assert_eq!((tall.width(), tall.height(), tall.top()), (640.0, 360.0, 320.0));
    }

    #[test]
    fn sprite_stands_on_its_y_and_is_centred_on_x() {
        let r = sprite_rect(stage(), 0.5, 1.0, 0.5, 0.5);
        assert_eq!((r.bottom(), r.height(), r.width(), r.center().x), (950.0, 450.0, 225.0, 900.0));
        let (x, y) = stage_point(stage(), pos2(900.0, 950.0));
        assert_eq!((x, y), (0.5, 1.0), "перетаскивание возвращает ту же позицию");
    }

    #[test]
    fn background_covers_without_distortion() {
        let uv = cover_uv(vec2(2000.0, 1000.0), stage());
        assert!((uv.width() - (16.0 / 9.0) / 2.0).abs() < 1e-6 && uv.height() == 1.0);
        let uv = cover_uv(vec2(1000.0, 1000.0), stage());
        assert!(uv.width() == 1.0 && (uv.height() - 9.0 / 16.0).abs() < 1e-6);
    }

    #[test]
    fn typewriter_hides_the_rest_but_keeps_the_full_text_for_layout() {
        let spans = vec![Span { text: "Привет, ".into(), bold: false }, Span { text: "мир".into(), bold: true }];
        assert_eq!(char_count(&spans), 11);
        let job = text_job(&spans, 9, 20.0, Color32::WHITE, TextStyle::Dialogue, 500.0);
        assert_eq!(job.text, "Привет, мир");
        let colors: Vec<Color32> = job.sections.iter().map(|s| s.format.color).collect();
        assert_eq!(colors, vec![Color32::WHITE, GOLD, Color32::TRANSPARENT]);
        let all = text_job(&spans, usize::MAX, 20.0, Color32::WHITE, TextStyle::Thought, 500.0);
        assert!(all.sections.iter().all(|s| s.format.color != Color32::TRANSPARENT && s.format.italics));
    }

    #[test]
    fn dialog_box_moves_with_its_setting() {
        let (top, mid, bottom) = (dialog_rect(stage(), DialogPos::Top), dialog_rect(stage(), DialogPos::Center), dialog_rect(stage(), DialogPos::Bottom));
        assert!(top.top() < mid.top() && mid.top() < bottom.top());
        assert!(stage().contains_rect(top) && stage().contains_rect(bottom));
    }
}
