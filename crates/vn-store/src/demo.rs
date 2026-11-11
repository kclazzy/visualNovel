//! Демонстрационный проект: небольшая новелла с ветками, переменными, музыкой и гардеробом.
//! Картинки и звуки рисуются и синтезируются кодом, поэтому демо не требует сторонних файлов.
use crate::fsutil::write_atomic;
use crate::project;
use image::{Rgba, RgbaImage};
use std::path::Path;
use vn_core::cond::Cond;
use vn_core::model::*;

type Color = [u8; 3];

fn px(c: Color) -> Rgba<u8> {
    Rgba([c[0], c[1], c[2], 255])
}

fn rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: Color) {
    for yy in y.max(0)..(y + h).min(img.height() as i32) {
        for xx in x.max(0)..(x + w).min(img.width() as i32) {
            img.put_pixel(xx as u32, yy as u32, px(c));
        }
    }
}

fn ellipse(img: &mut RgbaImage, cx: i32, cy: i32, rx: i32, ry: i32, c: Color) {
    for yy in (cy - ry).max(0)..(cy + ry).min(img.height() as i32) {
        for xx in (cx - rx).max(0)..(cx + rx).min(img.width() as i32) {
            let (dx, dy) = ((xx - cx) as f32 / rx as f32, (yy - cy) as f32 / ry as f32);
            if dx * dx + dy * dy <= 1.0 {
                img.put_pixel(xx as u32, yy as u32, px(c));
            }
        }
    }
}

fn gradient(w: u32, h: u32, top: Color, bottom: Color) -> RgbaImage {
    RgbaImage::from_fn(w, h, |_, y| {
        let t = y as f32 / h as f32;
        px(std::array::from_fn(|i| (top[i] as f32 + (bottom[i] as f32 - top[i] as f32) * t) as u8))
    })
}

const W: u32 = 1280;
const H: u32 = 720;

fn backgrounds() -> Vec<(&'static str, RgbaImage)> {
    let mut room = gradient(W, H, [84, 62, 78], [150, 110, 96]);
    rect(&mut room, 0, 560, 1280, 160, [92, 64, 52]);
    rect(&mut room, 780, 120, 320, 260, [250, 226, 160]);
    rect(&mut room, 932, 120, 16, 260, [84, 62, 78]);
    rect(&mut room, 140, 330, 380, 230, [60, 46, 70]);

    let mut street = gradient(W, H, [110, 170, 235], [220, 236, 250]);
    rect(&mut street, 0, 520, 1280, 200, [120, 124, 136]);
    for (i, x) in [60, 380, 760, 1040].into_iter().enumerate() {
        rect(&mut street, x, 220 - i as i32 * 20, 200, 300 + i as i32 * 20, [196 - i as u8 * 12, 150, 132]);
        rect(&mut street, x + 40, 300, 46, 60, [250, 240, 190]);
        rect(&mut street, x + 114, 300, 46, 60, [90, 110, 150]);
    }

    let mut cafe = gradient(W, H, [70, 46, 40], [130, 88, 60]);
    rect(&mut cafe, 0, 470, 1280, 250, [58, 36, 30]);
    rect(&mut cafe, 0, 430, 1280, 46, [176, 122, 74]);
    for x in [180, 520, 860, 1100] {
        ellipse(&mut cafe, x, 130, 46, 30, [255, 216, 130]);
        rect(&mut cafe, x - 3, 0, 6, 110, [40, 26, 22]);
    }

    let mut quay = gradient(W, H, [250, 150, 110], [70, 90, 150]);
    ellipse(&mut quay, 940, 300, 70, 70, [255, 226, 160]);
    rect(&mut quay, 0, 420, 1280, 300, [44, 66, 120]);
    rect(&mut quay, 0, 600, 1280, 120, [96, 88, 92]);

    let mut tower = gradient(W, H, [10, 14, 40], [36, 44, 86]);
    for (x, y) in [(120, 80), (300, 180), (520, 60), (700, 140), (1100, 90), (980, 220), (220, 300)] {
        ellipse(&mut tower, x, y, 3, 3, [255, 255, 230]);
    }
    rect(&mut tower, 0, 600, 1280, 120, [18, 22, 40]);
    rect(&mut tower, 590, 220, 100, 400, [226, 226, 232]);
    rect(&mut tower, 590, 330, 100, 50, [190, 60, 60]);
    rect(&mut tower, 590, 470, 100, 50, [190, 60, 60]);
    rect(&mut tower, 572, 170, 136, 54, [60, 60, 76]);

    let mut lit = tower.clone();
    ellipse(&mut lit, 640, 196, 250, 60, [255, 240, 170]);
    ellipse(&mut lit, 640, 196, 40, 26, [255, 255, 240]);

    vec![("room", room), ("street", street), ("cafe", cafe), ("quay", quay), ("lighthouse", tower), ("lighthouse_lit", lit)]
}

const SW: u32 = 400;
const SH: u32 = 900;
const SKIN: Color = [255, 220, 196];

fn layer() -> RgbaImage {
    RgbaImage::new(SW, SH)
}

/// Фигура: голова, шея, руки, туловище и ноги одного цвета кожи, с простым лицом.
fn body(mouth_down: bool, brows: bool) -> RgbaImage {
    let mut img = layer();
    rect(&mut img, 150, 430, 44, 400, SKIN);
    rect(&mut img, 206, 430, 44, 400, SKIN);
    rect(&mut img, 120, 200, 160, 260, SKIN);
    rect(&mut img, 84, 210, 36, 250, SKIN);
    rect(&mut img, 280, 210, 36, 250, SKIN);
    rect(&mut img, 184, 160, 32, 50, SKIN);
    ellipse(&mut img, 200, 100, 68, 78, SKIN);
    ellipse(&mut img, 176, 96, 8, 10, [50, 50, 70]);
    ellipse(&mut img, 224, 96, 8, 10, [50, 50, 70]);
    if brows {
        rect(&mut img, 164, 74, 26, 6, [60, 40, 40]);
        rect(&mut img, 210, 74, 26, 6, [60, 40, 40]);
    }
    if mouth_down {
        rect(&mut img, 182, 136, 36, 6, [170, 70, 80]);
    } else {
        ellipse(&mut img, 200, 132, 20, 10, [200, 80, 90]);
        rect(&mut img, 176, 120, 48, 12, SKIN);
    }
    img
}

fn hair(img: &mut RgbaImage, c: Color, long: bool) {
    ellipse(img, 200, 62, 78, 56, c);
    rect(img, 124, 60, 22, 70, c);
    rect(img, 254, 60, 22, 70, c);
    if long {
        rect(img, 118, 100, 26, 190, c);
        rect(img, 256, 100, 26, 190, c);
    }
}

fn top(img: &mut RgbaImage, c: Color, sleeves: bool) {
    rect(img, 116, 196, 168, 250, c);
    if sleeves {
        rect(img, 80, 204, 40, 200, c);
        rect(img, 280, 204, 40, 200, c);
    }
}

fn trousers(img: &mut RgbaImage, c: Color, long: bool) {
    let len = if long { 370 } else { 170 };
    rect(img, 116, 430, 168, 60, c);
    rect(img, 144, 470, 54, len - 40, c);
    rect(img, 202, 470, 54, len - 40, c);
}

fn shoes(img: &mut RgbaImage, c: Color) {
    rect(img, 136, 820, 66, 44, c);
    rect(img, 198, 820, 66, 44, c);
}

fn dressed(mouth_down: bool, brows: bool, hair_color: Color, long_hair: bool, top_color: Color, bottom: Color) -> RgbaImage {
    let mut img = body(mouth_down, brows);
    top(&mut img, top_color, true);
    trousers(&mut img, bottom, !long_hair);
    if long_hair {
        rect(&mut img, 110, 430, 180, 190, top_color);
    }
    shoes(&mut img, [40, 40, 50]);
    hair(&mut img, hair_color, long_hair);
    img
}

fn made<F: FnOnce(&mut RgbaImage)>(draw: F) -> RgbaImage {
    let mut img = layer();
    draw(&mut img);
    img
}

/// Короткая мелодия из синусов в WAV (моно, 16 бит).
fn wav(notes: &[f32], note_seconds: f32, volume: f32) -> Vec<u8> {
    const RATE: u32 = 11_025;
    let per = (RATE as f32 * note_seconds) as usize;
    let mut samples = Vec::with_capacity(per * notes.len());
    for &freq in notes {
        for i in 0..per {
            let t = i as f32 / RATE as f32;
            let envelope = (i as f32 / per as f32 * std::f32::consts::PI).sin();
            let s = (t * freq * std::f32::consts::TAU).sin() * 0.7 + (t * freq * 2.0 * std::f32::consts::TAU).sin() * 0.3;
            samples.push((s * envelope * volume * i16::MAX as f32) as i16);
        }
    }
    let data = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend(b"RIFF");
    out.extend((36 + data).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(RATE.to_le_bytes());
    out.extend((RATE * 2).to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend(data.to_le_bytes());
    out.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
    out
}

fn png(img: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

struct Builder {
    story: Story,
}

impl Builder {
    fn say(&self, who: Option<&str>, text: &str) -> Event {
        let speaker = who.and_then(|n| self.story.characters.iter().find(|c| c.name == n)).map(|c| c.id.clone());
        let style = if speaker.is_some() { TextStyle::Dialogue } else { TextStyle::Narration };
        Event::new(EventKind::Say { speaker, style, text: text.into(), expression: None, speed: None, sound: None, look: TextLook::default() })
    }
    fn think(&self, text: &str) -> Event {
        Event::new(EventKind::Say { speaker: self.story.wardrobe.hero.clone(), style: TextStyle::Thought, text: text.into(), expression: None, speed: None, sound: None, look: TextLook::default() })
    }
    fn who(&self, name: &str) -> CharId {
        self.story.characters.iter().find(|c| c.name == name).map(|c| c.id.clone()).unwrap_or_default()
    }
    fn show(&self, name: &str, expression: &str, pos: Pos) -> Event {
        Event::new(EventKind::ShowCharacter { character: self.who(name), expression: expression.into(), pos, fade: 0.4 })
    }
    fn hide(&self, name: &str) -> Event {
        Event::new(EventKind::HideCharacter { character: self.who(name), fade: 0.3 })
    }
    fn item(&self, name: &str) -> ItemId {
        self.story.wardrobe.items.iter().find(|i| i.name == name).map(|i| i.id.clone()).unwrap_or_default()
    }
}

fn bg(name: &str) -> Event {
    Event::new(EventKind::Background { image: format!("assets/backgrounds/{name}.png"), fade: 0.6 })
}
fn music(name: &str) -> Event {
    Event::new(EventKind::PlayMusic { track: format!("assets/music/{name}.wav"), volume: 0.6, fade: 1.0, looped: true })
}
fn sound(name: &str) -> Event {
    Event::new(EventKind::Sound { file: format!("assets/sounds/{name}.wav"), volume: 0.9 })
}
fn goto(scene: &Scene) -> Event {
    Event::new(EventKind::Goto { scene: scene.id.clone() })
}
fn end(title: &str) -> Event {
    Event::new(EventKind::End { title: title.into() })
}
fn set(var: &str, action: VarAction, value: Value) -> VarOp {
    VarOp { var: var.into(), action, value }
}
fn option(text: &str, to: Option<&Scene>, effects: Vec<VarOp>, cond: Option<&str>) -> ChoiceOption {
    ChoiceOption { id: OptionId::new("op"), text: text.into(), cond: cond.map(|c| Cond::parse(c).expect("условие демо")), effects, goto: to.map(|s| s.id.clone()) }
}
fn choice(prompt: &str, options: Vec<ChoiceOption>) -> Event {
    Event::new(EventKind::Choice { prompt: prompt.into(), options })
}
fn fade(out: bool) -> Event {
    Event::new(EventKind::Fade { out, duration: 0.5 })
}

/// Создать демо-проект в папке `dir`.
pub fn create(dir: &Path) -> Result<Story, String> {
    let mut story = project::create(dir, "Свет маяка")?;
    let io = |e: std::io::Error| e.to_string();
    for (name, img) in backgrounds() {
        write_atomic(&dir.join(format!("assets/backgrounds/{name}.png")), &png(&img)?).map_err(io)?;
    }
    let sprites = [
        ("characters/anna_smile", dressed(false, false, [150, 70, 60], true, [230, 120, 150], [230, 120, 150])),
        ("characters/anna_sad", dressed(true, true, [150, 70, 60], true, [230, 120, 150], [230, 120, 150])),
        ("characters/max_calm", dressed(true, false, [40, 40, 50], false, [70, 110, 90], [50, 54, 80])),
        ("characters/max_smile", dressed(false, false, [40, 40, 50], false, [70, 110, 90], [50, 54, 80])),
        ("characters/hero_body", body(false, false)),
        ("clothing/hair_short", made(|i| hair(i, [90, 60, 40], false))),
        ("clothing/hair_long", made(|i| hair(i, [230, 190, 90], true))),
        ("clothing/top_tshirt", made(|i| top(i, [240, 240, 244], false))),
        ("clothing/top_hoodie", made(|i| top(i, [70, 130, 200], true))),
        ("clothing/top_jacket", made(|i| {
            top(i, [200, 60, 60], true);
            rect(i, 194, 196, 12, 250, [250, 220, 120]);
        })),
        ("clothing/bottom_jeans", made(|i| trousers(i, [70, 100, 170], true))),
        ("clothing/bottom_shorts", made(|i| trousers(i, [110, 90, 70], false))),
        ("clothing/shoes_sneakers", made(|i| shoes(i, [240, 240, 240]))),
        ("clothing/shoes_boots", made(|i| {
            shoes(i, [80, 50, 30]);
            rect(i, 144, 770, 54, 60, [80, 50, 30]);
            rect(i, 202, 770, 54, 60, [80, 50, 30]);
        })),
        ("clothing/scarf", made(|i| {
            rect(i, 150, 166, 100, 40, [250, 200, 70]);
            rect(i, 226, 200, 30, 110, [250, 200, 70]);
        })),
    ];
    for (name, img) in &sprites {
        write_atomic(&dir.join(format!("assets/{name}.png")), &png(img)?).map_err(io)?;
    }
    let sounds = [
        ("music/calm", wav(&[262.0, 330.0, 392.0, 330.0, 294.0, 349.0, 440.0, 349.0], 0.75, 0.35)),
        ("music/night", wav(&[220.0, 262.0, 330.0, 262.0, 196.0, 247.0, 294.0, 247.0], 0.9, 0.35)),
        ("sounds/bell", wav(&[880.0, 1175.0], 0.18, 0.5)),
        ("sounds/switch", wav(&[180.0], 0.12, 0.7)),
    ];
    for (name, bytes) in &sounds {
        write_atomic(&dir.join(format!("assets/{name}.wav")), bytes).map_err(io)?;
    }

    story.meta.description = "Демонстрационная новелла: ветки, переменные, музыка и гардероб.".into();
    story.meta.author = "Visual Novel Studio".into();
    let face = |name: &str, file: &str| Expression { name: name.into(), image: format!("assets/characters/{file}.png") };
    let person = |name: &str, about: &str, color: Color, expressions: Vec<Expression>| Character { id: CharId::new("ch"), name: name.into(), description: about.into(), color, expressions, height: 0.92 };
    story.characters = vec![
        person("Анна", "Хозяйка кафе у набережной, знает все истории города.", [255, 170, 190], vec![face("улыбка", "anna_smile"), face("грусть", "anna_sad")]),
        person("Макс", "Немногословный бариста, бывший смотритель маяка.", [150, 220, 180], vec![face("спокоен", "max_calm"), face("улыбка", "max_smile")]),
        person("Саша", "Главный герой. Только что приехал в город.", [140, 200, 255], vec![face("обычное", "hero_body")]),
    ];
    let hero = story.characters[2].id.clone();
    let slot = |name: &str, behind: bool| Slot { id: SlotId::new("slot"), name: name.into(), behind };
    let slots = vec![slot("Низ", false), slot("Обувь", false), slot("Верх", false), slot("Причёска", false), slot("Аксессуар", false)];
    let wear = |slot: usize, name: &str, file: &str, unlocked: bool| Item { id: ItemId::new("it"), slot: slots[slot].id.clone(), name: name.into(), image: format!("assets/clothing/{file}.png"), unlocked };
    story.wardrobe = Wardrobe {
        hero: Some(hero),
        items: vec![
            wear(0, "Джинсы", "bottom_jeans", true),
            wear(0, "Шорты", "bottom_shorts", true),
            wear(1, "Кеды", "shoes_sneakers", true),
            wear(1, "Ботинки", "shoes_boots", true),
            wear(2, "Футболка", "top_tshirt", true),
            wear(2, "Худи", "top_hoodie", true),
            wear(2, "Куртка смотрителя", "top_jacket", false),
            wear(3, "Короткая стрижка", "hair_short", true),
            wear(3, "Длинные волосы", "hair_long", true),
            wear(4, "Шарф Анны", "scarf", false),
        ],
        slots,
        anytime: true,
    };
    story.variables = vec![
        VarDef { name: "доверие".into(), initial: Value::Int(0), description: "Насколько Анна доверяет герою".into() },
        VarDef { name: "деньги".into(), initial: Value::Int(100), description: "Монеты в кармане".into() },
        VarDef { name: "знает_о_маяке".into(), initial: Value::Bool(false), description: "Герой слышал историю маяка".into() },
        VarDef { name: "имя".into(), initial: Value::Text("Саша".into()), description: "Имя героя в тексте".into() },
    ];
    story.chapters = ["Глава 1. Приезд", "Глава 2. Кафе", "Глава 3. Маяк"].into_iter().map(|n| Chapter { id: ChapterId::new("chp"), name: n.into() }).collect();

    let b = Builder { story };
    let chapter = |n: usize| Some(b.story.chapters[n].id.clone());
    let new_scene = |name: &str, ch: usize| {
        let mut s = Scene::new(name);
        s.chapter = chapter(ch);
        s
    };
    let mut prologue = b.story.scenes[0].clone();
    prologue.name = "Комната".into();
    prologue.chapter = chapter(0);
    let [mut street, mut cafe_anna, mut cafe_alone, mut quay, mut legend, mut farewell, mut tower, mut summit, mut lit, mut dark, mut home] =
        [("Улица", 0), ("Кафе с Анной", 1), ("Кафе в одиночку", 1), ("Набережная", 1), ("Легенда о маяке", 1), ("Прощание", 1), ("У маяка", 2), ("На вершине", 2), ("Свет", 2), ("Темнота", 2), ("Дом", 1)].map(|(n, c)| new_scene(n, c));

    let (dressed_hero, jeans, sneakers, tshirt, short_hair) = (b.story.wardrobe.slots.clone(), b.item("Джинсы"), b.item("Кеды"), b.item("Футболка"), b.item("Короткая стрижка"));
    let outfit = |slot: usize, item: &ItemId| Event::new(EventKind::SetOutfit { slot: dressed_hero[slot].id.clone(), item: Some(item.clone()) });
    prologue.events = vec![
        outfit(0, &jeans),
        outfit(1, &sneakers),
        outfit(2, &tshirt),
        outfit(3, &short_hair),
        bg("room"),
        music("calm"),
        b.say(None, "Первое утро в приморском городе. За окном кричат чайки."),
        b.show("Саша", "обычное", Pos::CENTER),
        b.think("Новый город, новая жизнь. С чего бы начать, {имя}?"),
        b.say(None, "Сначала стоит одеться. *Выберите образ героя* — его можно сменить в любой момент из меню."),
        Event::new(EventKind::OpenWardrobe { slots: vec![] }),
        b.think("Так-то лучше. Пора на улицу."),
        goto(&street),
    ];
    street.events = vec![
        fade(true),
        bg("street"),
        b.hide("Саша"),
        fade(false),
        b.say(None, "У дверей кафе девушка пытается удержать башню из коробок."),
        b.show("Анна", "грусть", Pos::RIGHT),
        b.say(Some("Анна"), "Ой-ой-ой… Только не падайте!"),
        choice(
            "Как поступить?",
            vec![
                option("Помочь донести коробки", Some(&cafe_anna), vec![set("доверие", VarAction::Add, Value::Int(2))], None),
                option("Извиниться и пройти мимо", Some(&cafe_alone), vec![], None),
                option("Молча придержать дверь", None, vec![set("доверие", VarAction::Add, Value::Int(1))], None),
            ],
        ),
        Event::new(EventKind::Expression { character: b.who("Анна"), expression: "улыбка".into() }),
        b.say(Some("Анна"), "Спасибо! Заходите как-нибудь на кофе."),
        b.say(None, "Она скрылась за дверью. Что ж, кофе — хорошая мысль."),
        goto(&cafe_anna),
    ];
    cafe_anna.events = vec![
        bg("cafe"),
        sound("bell"),
        b.show("Анна", "улыбка", Pos::LEFT),
        b.say(Some("Анна"), "Добро пожаловать в «Маяк»! Я Анна. А вы, должно быть, {имя}?"),
        b.show("Макс", "спокоен", Pos::RIGHT),
        b.say(Some("Макс"), "Кофе готов. С вас пятьдесят монет."),
        b.think("В кармане {деньги} монет."),
        choice(
            "",
            vec![
                option("Заплатить за всех (50 монет)", None, vec![set("деньги", VarAction::Sub, Value::Int(50)), set("доверие", VarAction::Add, Value::Int(1))], Some("деньги >= 50")),
                option("Рассказать смешную историю", None, vec![set("доверие", VarAction::Add, Value::Int(1))], None),
                option("Молча пить кофе", None, vec![], None),
            ],
        ),
        Event::new(EventKind::Expression { character: b.who("Макс"), expression: "улыбка".into() }),
        b.say(Some("Макс"), "Хм. Неплохо для новичка."),
        goto(&quay),
    ];
    cafe_alone.events = vec![
        bg("cafe"),
        sound("bell"),
        b.show("Макс", "спокоен", Pos::CENTER),
        b.say(Some("Макс"), "Кофе? Садитесь у окна, оттуда виден маяк."),
        b.say(None, "Старый маяк на мысе давно не горит."),
        choice(
            "",
            vec![
                option("Спросить про маяк", Some(&tower), vec![set("знает_о_маяке", VarAction::Set, Value::Bool(true))], None),
                option("Допить кофе и пойти домой", Some(&home), vec![], None),
            ],
        ),
    ];
    quay.events = vec![
        fade(true),
        bg("quay"),
        b.hide("Макс"),
        fade(false),
        Event::new(EventKind::MoveCharacter { character: b.who("Анна"), pos: Pos::CENTER, duration: 0.8 }),
        b.say(None, "Вечером Анна предложила пройтись по набережной."),
        Event::new(EventKind::Branch { cond: Cond::parse("доверие >= 2").expect("условие демо"), scene: legend.id.clone() }),
        goto(&farewell),
    ];
    legend.events = vec![
        b.say(Some("Анна"), "Знаете, {имя}, маяк погас в ту ночь, когда ушёл его смотритель."),
        b.say(Some("Анна"), "Макс до сих пор хранит его куртку. Возьмите — и *мой шарф* тоже, наверху ветрено."),
        Event::new(EventKind::UnlockItem { item: b.item("Куртка смотрителя") }),
        Event::new(EventKind::UnlockItem { item: b.item("Шарф Анны") }),
        Event::new(EventKind::SetVar(set("знает_о_маяке", VarAction::Set, Value::Bool(true)))),
        b.say(None, "Получены предметы: «Куртка смотрителя» и «Шарф Анны»."),
        goto(&tower),
    ];
    farewell.events = vec![
        Event::new(EventKind::Expression { character: b.who("Анна"), expression: "грусть".into() }),
        b.say(Some("Анна"), "Красивый закат… Ну, мне пора закрывать кафе."),
        b.think("Кажется, она хотела сказать что-то ещё."),
        Event::new(EventKind::StopMusic { fade: 1.5 }),
        end("Недосказанность"),
    ];
    home.events = vec![fade(true), bg("room"), b.hide("Макс"), fade(false), b.say(None, "Обычный вечер в новом городе. Маяк за окном так и остался тёмным."), Event::new(EventKind::StopMusic { fade: 1.5 }), end("Обычный вечер")];
    tower.events = vec![
        fade(true),
        bg("lighthouse"),
        b.hide("Анна"),
        b.hide("Макс"),
        music("night"),
        fade(false),
        Event::new(EventKind::DialogBox { pos: DialogPos::Top }),
        b.say(None, "Ночь. Маяк возвышается над мысом тёмной башней."),
        Event::new(EventKind::DialogBox { pos: DialogPos::Bottom }),
        b.show("Саша", "обычное", Pos::LEFT),
        b.think("Дверь не заперта. Подняться?"),
        choice("", vec![option("Подняться наверх", Some(&summit), vec![], None), option("Вернуться домой", Some(&home), vec![], None)]),
    ];
    summit.events = vec![
        b.say(None, "Наверху гуляет ветер. Самое время надеть что-нибудь тёплое."),
        Event::new(EventKind::OpenWardrobe { slots: vec![b.story.wardrobe.slots[2].id.clone(), b.story.wardrobe.slots[4].id.clone()] }),
        b.say(None, "Перед вами старый рубильник."),
        choice("Что сделать?", vec![option("Зажечь маяк", Some(&lit), vec![], Some("знает_о_маяке")), option("Постоять в темноте", Some(&dark), vec![], None)]),
    ];
    let mut flash = Event::new(EventKind::Background { image: "assets/backgrounds/lighthouse_lit.png".into(), fade: 1.2 });
    flash.delay = 0.6;
    lit.events = vec![sound("switch"), flash, b.say(None, "Луч прорезал ночь. Где-то внизу, у кафе, зажглось окно."), b.think("Кажется, я здесь надолго."), Event::new(EventKind::StopMusic { fade: 2.0 }), end("Свет маяка")];
    dark.events = vec![Event::new(EventKind::Wait { seconds: 1.0 }), b.say(None, "Звёзды над морем и без маяка были хороши."), Event::new(EventKind::StopMusic { fade: 2.0 }), end("Тихая ночь")];

    let mut story = b.story;
    let scenes = vec![prologue, street, cafe_anna, cafe_alone, quay, legend, farewell, home, tower, summit, lit, dark];
    let grid: [[f32; 2]; 12] = [[0.0, 150.0], [240.0, 150.0], [480.0, 40.0], [480.0, 300.0], [720.0, 40.0], [960.0, -60.0], [960.0, 90.0], [720.0, 430.0], [1200.0, 220.0], [1440.0, 220.0], [1680.0, 120.0], [1680.0, 330.0]];
    story.layout = scenes.iter().zip(grid).map(|(s, p)| (s.id.clone(), p)).collect();
    story.scenes = scenes;
    project::save(dir, &story)?;
    Ok(story)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vn_core::analysis::endings;
    use vn_core::runtime::{Pending, Runtime};
    use vn_core::validate::validate;

    #[test]
    fn demo_project_is_valid_complete_and_meets_the_readiness_criteria() {
        let tmp = tempfile::tempdir().unwrap();
        let story = create(tmp.path()).unwrap();
        assert_eq!(project::load(tmp.path()).unwrap().story, story);
        let issues = validate(&story, &project::list_assets(tmp.path()));
        assert_eq!(issues, vec![], "демо проходит проверку без ошибок и предупреждений");
        assert!(story.scenes.len() >= 10 && story.characters.len() >= 3 && story.variables.len() >= 3);
        let ends = endings(&story);
        assert_eq!(ends.len(), 4);
        assert!(ends.iter().all(|e| e.reachable));
    }

    /// Пройти историю, всегда выбирая вариант с данным номером (или последний доступный).
    fn playthrough(story: &Story, pick: usize) -> String {
        let mut rt = Runtime::new(story.clone());
        for _ in 0..500 {
            match rt.pending().clone() {
                Pending::Ended(title) => return title,
                Pending::Choice => {
                    let options = rt.options();
                    let id = options[pick.min(options.len() - 1)].id.clone();
                    assert!(rt.choose(&id));
                }
                Pending::Error(e) => panic!("{e}"),
                _ => rt.advance(),
            }
        }
        panic!("история не закончилась");
    }

    #[test]
    fn every_ending_of_the_demo_can_be_played_to() {
        let tmp = tempfile::tempdir().unwrap();
        let story = create(tmp.path()).unwrap();
        let reached: std::collections::BTreeSet<String> = (0..3).map(|pick| playthrough(&story, pick)).collect();
        assert!(reached.contains("Свет маяка"), "{reached:?}");
        assert!(reached.len() >= 3, "разные выборы ведут к разным концовкам: {reached:?}");
    }

    #[test]
    fn demo_builds_for_desktop_and_phone() {
        let tmp = tempfile::tempdir().unwrap();
        let story = create(tmp.path()).unwrap();
        let player = tmp.path().join("vn-player.exe");
        std::fs::write(&player, b"MZ").unwrap();
        let game = crate::export::export_desktop(tmp.path(), &story, &player).unwrap();
        let pack = crate::pack::Pack::open(std::fs::read(game.join(crate::export::PACK_FILE)).unwrap()).unwrap();
        assert!(story.assets_used().iter().all(|a| pack.asset(a).is_some()));
        let web = crate::export::export_web(tmp.path(), &story).unwrap();
        assert!(story.assets_used().iter().all(|a| web.join(a).is_file()));
    }

    #[test]
    fn generated_sound_is_a_wav_file() {
        let bytes = wav(&[440.0], 0.1, 0.5);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 1102 * 2);
    }
}
