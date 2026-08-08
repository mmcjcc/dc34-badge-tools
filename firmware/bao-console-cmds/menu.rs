//! Badge menu, LED patterns, sleep, and a second game.
//!
//! Buttons (from bao1x-hal board/baosec.rs kpc_sr0_to_key, mapped to chars by
//! the keyboard service):
//!     '\u{2190}' Left   '\u{2192}' Right   '\u{1F525}' Center/fire
//!     '\u{2191}' Up     '\u{2193}' Down    '\u{2234}' Select  <- opens the menu
//!
//! LED facts (bunnie/dc34-console/src/leds.rs): BIO pin 15, 10 pixels, index
//! 0,1 = eyes, 2..9 = ring, WS2812C-2020, GRB.

pub mod font;

pub const W: isize = 128;
pub const H: isize = 128;
pub const LED_N: usize = 10;
pub const LED_PIN: u8 = 15;

// ---------------------------------------------------------------- framebuffer

/// A set bit renders DARK on this panel, so "lit" clears the bit.
pub fn px(fb: &mut [u32; 512], x: isize, y: isize, lit: bool) {
    if x < 0 || x >= W || y < 0 || y >= H {
        return;
    }
    let bn = (x + y * W) as usize;
    if lit {
        fb[bn >> 5] &= !(1u32 << (bn & 31));
    } else {
        fb[bn >> 5] |= 1u32 << (bn & 31);
    }
}

pub fn clear(fb: &mut [u32; 512]) { fb.iter_mut().for_each(|w| *w = 0xFFFF_FFFF); }

/// Draw one 5x7 glyph, `scale`x. Unknown chars render blank.
pub fn glyph(fb: &mut [u32; 512], ch: char, x: isize, y: isize, scale: isize) {
    let up = ch.to_ascii_uppercase();
    let idx = match font::FONT_CHARS.chars().position(|c| c == up) {
        Some(i) => i,
        None => return,
    };
    let cols = &font::FONT[idx];
    for (cx, col) in cols.iter().enumerate() {
        for row in 0..7 {
            if (col >> row) & 1 == 0 {
                continue;
            }
            for dy in 0..scale {
                for dx in 0..scale {
                    px(fb, x + cx as isize * scale + dx, y + row as isize * scale + dy, true);
                }
            }
        }
    }
}

pub fn text(fb: &mut [u32; 512], s: &str, x: isize, y: isize, scale: isize) {
    let mut cx = x;
    for ch in s.chars() {
        glyph(fb, ch, cx, y, scale);
        cx += 6 * scale;
    }
}

/// Invert a rectangle -- used to highlight the selected menu row.
pub fn invert(fb: &mut [u32; 512], x0: isize, y0: isize, x1: isize, y1: isize) {
    for y in y0..y1 {
        for x in x0..x1 {
            if x < 0 || x >= W || y < 0 || y >= H {
                continue;
            }
            let bn = (x + y * W) as usize;
            fb[bn >> 5] ^= 1u32 << (bn & 31);
        }
    }
}

// ---------------------------------------------------------------------- LEDs

#[derive(Clone, Copy, PartialEq)]
pub enum LedMode {
    GameReactive,
    Rainbow,
    Chase,
    Blink,
    Sos,
    Random,
    Off,
}

impl LedMode {
    pub fn label(&self) -> &'static str {
        match self {
            LedMode::GameReactive => "LEDS: GAME",
            LedMode::Rainbow => "LEDS: RAINBOW",
            LedMode::Chase => "LEDS: CHASE",
            LedMode::Blink => "LEDS: BLINK",
            LedMode::Sos => "LEDS: SOS",
            LedMode::Random => "LEDS: RANDOM",
            LedMode::Off => "LEDS: OFF",
        }
    }
    pub fn next(&self) -> LedMode {
        match self {
            LedMode::GameReactive => LedMode::Rainbow,
            LedMode::Rainbow => LedMode::Chase,
            LedMode::Chase => LedMode::Blink,
            LedMode::Blink => LedMode::Sos,
            LedMode::Sos => LedMode::Random,
            LedMode::Random => LedMode::Off,
            LedMode::Off => LedMode::GameReactive,
        }
    }
}

/// Integer HSV -> RGB, value scaled down: these sit under your chin and run
/// from two AA cells.
pub fn hsv(h: u8, v: u8) -> (u8, u8, u8) {
    let region = h / 43;
    let f = ((h as u16 % 43) * 6) as u8;
    let q = v.saturating_sub((v as u16 * f as u16 / 255) as u8);
    let t = (v as u16 * f as u16 / 255) as u8;
    match region {
        0 => (v, t, 0),
        1 => (q, v, 0),
        2 => (0, v, t),
        3 => (0, q, v),
        4 => (t, 0, v),
        _ => (v, 0, q),
    }
}

/// SOS in morse: 3 short, 3 long, 3 short, then a gap. `t` counts frames.
pub fn sos_on(t: u32) -> bool {
    // unit = 3 frames (~300ms at 100ms/frame)
    const U: u32 = 3;
    // ... --- ...  then a word gap
    let seq: [(u32, bool); 18] = [
        (U, true), (U, false), (U, true), (U, false), (U, true), (U * 3, false),
        (U * 3, true), (U, false), (U * 3, true), (U, false), (U * 3, true), (U * 3, false),
        (U, true), (U, false), (U, true), (U, false), (U, true), (U * 7, false),
    ];
    let total: u32 = seq.iter().map(|(d, _)| *d).sum();
    let mut pos = t % total;
    for (d, on) in seq.iter() {
        if pos < *d {
            return *on;
        }
        pos -= *d;
    }
    false
}

/// Cheap integer hash -- good enough for sparkle, and deterministic so the
/// pattern is a pure function of the frame counter.
fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

/// RANDOM mode: re-rolls scene, palette and speed every few seconds so the
/// badge never looks like it is doing the same thing twice.
///
/// Scenes:
///   0 sparkle  - random pixels pop, others dim in a base hue
///   1 wave     - hue gradient sweeping the ring at a random rate/direction
///   2 strobe   - the whole strip pulses a random colour
///   3 twinkle  - each pixel has its own slow, offset brightness cycle
fn random_frame(t: u32) -> [(u8, u8, u8); LED_N] {
    const SCENE_FRAMES: u32 = 30; // ~3s at 100ms/frame
    let epoch = t / SCENE_FRAMES;
    let r = hash32(epoch);
    let scene = r % 4;
    let base_hue = (r >> 8) as u8;
    let speed = 1 + ((r >> 16) % 4); // 1..4
    let dir: i32 = if (r >> 20) & 1 == 0 { 1 } else { -1 };
    let local = t % SCENE_FRAMES;

    let mut s = [(0u8, 0u8, 0u8); LED_N];
    match scene {
        0 => {
            // sparkle: a couple of bright pops over a dim wash
            let (br, bg, bb) = hsv(base_hue, 18);
            for p in s.iter_mut() {
                *p = (br, bg, bb);
            }
            for i in 0..LED_N {
                let h = hash32(t.wrapping_mul(31).wrapping_add(i as u32));
                if h % 11 == 0 {
                    let (r2, g2, b2) = hsv((h >> 8) as u8, 140);
                    s[i] = (r2, g2, b2);
                }
            }
        }
        1 => {
            // wave: hue gradient rotating around the strip
            for (i, p) in s.iter_mut().enumerate() {
                let phase = (t as i32 * dir * speed as i32) as i32 + (i as i32 * (256 / LED_N as i32));
                *p = hsv((base_hue as i32).wrapping_add(phase) as u8, 70);
            }
        }
        2 => {
            // strobe: whole strip pulses, rate from `speed`
            let on = (t / (6 - speed).max(1)) % 2 == 0;
            if on {
                let (r2, g2, b2) = hsv(base_hue, 120);
                for p in s.iter_mut() {
                    *p = (r2, g2, b2);
                }
            }
        }
        _ => {
            // twinkle: independent slow cycles, offset per pixel
            for (i, p) in s.iter_mut().enumerate() {
                let off = hash32(epoch.wrapping_add(i as u32 * 7)) % 64;
                let ph = (t.wrapping_mul(speed).wrapping_add(off)) % 64;
                let v = if ph < 32 { ph * 3 } else { (64 - ph) * 3 } as u8;
                let (r2, g2, b2) = hsv(base_hue.wrapping_add((i as u32 * 12) as u8), v.min(110));
                *p = (r2, g2, b2);
            }
        }
    }
    let _ = local;
    s
}

/// Build the 10-pixel strip for the current mode.
pub fn led_frame(mode: LedMode, t: u32, flash: bool) -> [(u8, u8, u8); LED_N] {
    let mut s = [(0u8, 0u8, 0u8); LED_N];
    match mode {
        LedMode::Off => {}
        LedMode::GameReactive => {
            if flash {
                s = [(255, 40, 0); LED_N];
            } else {
                let (r, g, b) = hsv((t / 2) as u8, 30);
                s = [(r, g, b); LED_N];
            }
        }
        LedMode::Rainbow => {
            for (i, p) in s.iter_mut().enumerate() {
                let h = ((t * 2) as usize + i * (256 / LED_N)) as u8;
                *p = hsv(h, 60);
            }
        }
        LedMode::Chase => {
            // eyes stay dim, a bright dot runs around the 8-LED ring
            let head = ((t / 2) as usize) % (LED_N - 2);
            s[0] = (10, 10, 10);
            s[1] = (10, 10, 10);
            for i in 0..(LED_N - 2) {
                let d = if i >= head { i - head } else { (LED_N - 2) - head + i };
                let v = match d { 0 => 120u8, 1 => 40, 2 => 12, _ => 0 };
                s[i + 2] = (v, 0, v / 3);
            }
        }
        LedMode::Blink => {
            let on = (t / 5) % 2 == 0;
            if on {
                s = [(90, 90, 90); LED_N];
            }
        }
        LedMode::Random => {
            s = random_frame(t);
        }
        LedMode::Sos => {
            if sos_on(t) {
                s = [(160, 160, 160); LED_N];
            }
        }
    }
    s
}

/// Build the driver ONCE. Constructing a Ws2812 per frame re-inits the BIO
/// core and re-sends the pin mask mid-stream, which tears up the data in
/// flight: the first LEDs in the chain (the eyes) still latch, while the rest
/// of the ring keeps its previous colour. That was the "ring stuck orange" bug.
#[cfg(feature = "bio-lib")]
pub fn led_open() -> Option<bio_lib::ws2812::Ws2812> {
    use arbitrary_int::u5;
    use bio_lib::ws2812::{LedVariant, Ws2812};
    Ws2812::new(LedVariant::C, u5::new(LED_PIN), None).ok()
}
#[cfg(not(feature = "bio-lib"))]
pub fn led_open() -> Option<()> { None }

#[cfg(feature = "bio-lib")]
pub fn led_push(ws: &mut Option<bio_lib::ws2812::Ws2812>, strip: &[(u8, u8, u8); LED_N]) {
    use bio_lib::ws2812::rgb_to_u32;
    if let Some(ws) = ws.as_mut() {
        let mut buf = [0u32; LED_N];
        for (i, (r, g, b)) in strip.iter().enumerate() {
            buf[i] = rgb_to_u32(*r, *g, *b);
        }
        // send(), NOT send_async(): FIFO1 is 8 deep and we push 10 words, so
        // without waiting for the completion token it jams and every later
        // frame is dropped -- the strip then sticks on one colour.
        ws.send(&buf);
    }
}
#[cfg(not(feature = "bio-lib"))]
pub fn led_push(_ws: &mut Option<()>, _strip: &[(u8, u8, u8); LED_N]) {}

// ---------------------------------------------------------------------- menu

#[derive(Clone, Copy, PartialEq)]
pub enum Item {
    Resume,
    Game,
    Leds,
    Bright,
    Sleep,
}

pub const ITEMS: [Item; 5] = [Item::Resume, Item::Game, Item::Leds, Item::Bright, Item::Sleep];

#[derive(Clone, Copy, PartialEq)]
pub enum GameKind {
    Invaders,
    AirSea,
}

impl GameKind {
    pub fn label(&self) -> &'static str {
        match self {
            GameKind::Invaders => "GAME: INVADERS",
            GameKind::AirSea => "GAME: AIR-SEA",
        }
    }
    pub fn next(&self) -> GameKind {
        match self {
            GameKind::Invaders => GameKind::AirSea,
            GameKind::AirSea => GameKind::Invaders,
        }
    }
}

pub fn draw_menu(
    fb: &mut [u32; 512],
    sel: usize,
    game: GameKind,
    leds: LedMode,
    bright: u8,
) {
    clear(fb);
    text(fb, "DC34 BADGE", 20, 6, 2);
    for x in 6..122 {
        px(fb, x, 24, true);
    }

    let mut y = 34;
    for (i, it) in ITEMS.iter().enumerate() {
        let mut buf = [0u8; 24];
        let label: &str = match it {
            Item::Resume => "RESUME",
            Item::Game => game.label(),
            Item::Leds => leds.label(),
            Item::Bright => {
                // render "BRIGHT: NN%" without alloc
                let pct = (bright as u16 * 100 / 255) as u8;
                let s = b"BRIGHT: ";
                buf[..s.len()].copy_from_slice(s);
                let mut n = s.len();
                if pct >= 100 {
                    buf[n] = b'1'; n += 1;
                    buf[n] = b'0'; n += 1;
                    buf[n] = b'0'; n += 1;
                } else {
                    if pct >= 10 {
                        buf[n] = b'0' + pct / 10; n += 1;
                    }
                    buf[n] = b'0' + pct % 10; n += 1;
                }
                buf[n] = b'%'; n += 1;
                core::str::from_utf8(&buf[..n]).unwrap_or("BRIGHT")
            }
            Item::Sleep => "SLEEP",
        };
        if i == sel {
            text(fb, ">", 6, y, 1);
        }
        text(fb, label, 16, y, 1);
        if i == sel {
            invert(fb, 4, y - 2, 124, y + 9);
        }
        y += 14;
    }

    text(fb, "SEL:OPEN  FIRE:PICK", 6, 116, 1);
}
