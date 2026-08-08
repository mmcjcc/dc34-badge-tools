use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use String;

use crate::{CommonEnv, ShellCmdApi};

/// Interactive Space Invaders on the badge's 128x128 1bpp OLED.
///
/// Controls: <- / -> move, center (fire) shoots, select quits.
///
/// TWO HARD-WON FACTS baked in here:
///  * The panel boots at ZERO BRIGHTNESS. gfx renders correctly and reports
///    success while you see nothing. Always call gfx.brightness() first.
///  * A set framebuffer bit renders DARK (clear() fills 0xFFFFFFFF), so "lit"
///    means CLEARING a bit. See px().
///
/// Input uses get_keys_blocking() on a helper thread, because it blocks; the
/// game loop runs on its own clock and just reads the shared atomics.
#[derive(Debug)]
pub struct Invaders {}

const W: isize = 128;
const H: isize = 128;
const BRIGHT: u8 = 200;

// 1978 sprite art, MSB-left, one u16 row per line.
const SQUID: [u16; 8] =
    [0b00011000, 0b00111100, 0b01111110, 0b11011011, 0b11111111, 0b00100100, 0b01011010, 0b10100101];
const CRAB: [u16; 8] = [
    0b00100000100, 0b00010001000, 0b00111111100, 0b01101110110,
    0b11111111111, 0b10111111101, 0b10100000101, 0b00011011000,
];
const OCTO: [u16; 8] = [
    0b00011111000, 0b01111111110, 0b11111111111, 0b11001110011,
    0b11111111111, 0b00110101100, 0b01100100110, 0b11000000011,
];
const CANNON: [u16; 8] = [
    0b0000001000000, 0b0000011100000, 0b0000011100000, 0b0111111111110,
    0b1111111111111, 0b1111111111111, 0b1111111111111, 0b1111111111111,
];

const COLS: usize = 5;
const ROWS: usize = 3;
const PITCH_X: isize = 23;
const PITCH_Y: isize = 18;
const SCALE: isize = 2;

fn sprite(row: usize) -> (&'static [u16; 8], isize) {
    match row {
        0 => (&SQUID, 8),
        1 => (&CRAB, 11),
        _ => (&OCTO, 11),
    }
}

/// lit=true clears the bit (bit 0 = lit on this panel).
fn px(fb: &mut [u32; 512], x: isize, y: isize, lit: bool) {
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

fn blit(fb: &mut [u32; 512], rows: &[u16; 8], w: isize, x: isize, y: isize) {
    for (ry, row) in rows.iter().enumerate() {
        for rx in 0..w {
            if (row >> (w - 1 - rx)) & 1 == 0 {
                continue;
            }
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    px(fb, x + rx * SCALE + dx, y + ry as isize * SCALE + dy, true);
                }
            }
        }
    }
}

/// Attract-mode AI: steer toward the nearest live alien column and fire when
/// roughly lined up. Deliberately imperfect -- it should look like play, not a
/// solver, and it will eventually lose, which restarts the demo.
fn autopilot(g: &Game) -> (isize, bool) {
    let mut target_x: Option<isize> = None;
    let mut best_y = -1isize;
    for r in 0..ROWS {
        for c in 0..COLS {
            if !g.alive[r][c] {
                continue;
            }
            let (x, y, w, _) = g.abox(r, c);
            // prefer the lowest (most dangerous) row
            if y > best_y {
                best_y = y;
                target_x = Some(x + w / 2);
            }
        }
    }
    let muzzle = g.player + 13 * SCALE / 2;
    match target_x {
        Some(tx) => {
            let d = if tx > muzzle + 3 { 1 } else if tx < muzzle - 3 { -1 } else { 0 };
            (d, d == 0)
        }
        None => (0, false),
    }
}

/// Badge LEDs: pin 15, 10 pixels (0,1 = eyes; 2..9 = ring), per
/// bunnie/dc34-console/src/leds.rs. Best-effort -- never block the game.
///
/// `ttl > 0` means a recent kill: flash orange. Otherwise idle on a slow hue
/// cycle so the badge looks alive when nothing is happening.
#[cfg(feature = "bio-lib")]
fn led_update(hue: u8, ttl: u8) {
    use arbitrary_int::u5;
    use bio_lib::ws2812::{LedVariant, Ws2812, rgb_to_u32};
    let (r, g, b) = if ttl > 0 {
        (255u8, 40u8, 0u8)
    } else {
        // integer HSV at low value -- easy on two AA cells
        match hue / 43 {
            0 => (40, hue.wrapping_mul(6) / 6, 0),
            1 => (40u8.saturating_sub(hue / 6), 40, 0),
            2 => (0, 40, hue / 6),
            3 => (0, 40u8.saturating_sub(hue / 6), 40),
            4 => (hue / 6, 0, 40),
            _ => (40, 0, 40u8.saturating_sub(hue / 6)),
        }
    };
    if let Ok(mut ws) = Ws2812::new(LedVariant::C, u5::new(15), None) {
        ws.send_async(&[rgb_to_u32(r, g, b); 10]);
    }
}
#[cfg(not(feature = "bio-lib"))]
fn led_update(_hue: u8, _ttl: u8) {}

struct Game {
    alive: [[bool; COLS]; ROWS],
    fx: isize,
    fy: isize,
    dir: isize,
    player: isize,
    shot: Option<(isize, isize)>,
    bomb: Option<(isize, isize)>,
    score: u32,
    over: bool,
    tick: u32,
    rng: u32,
}

impl Game {
    fn new(seed: u32) -> Self {
        Game {
            alive: [[true; COLS]; ROWS],
            fx: 6,
            fy: 26,
            dir: 1,
            player: W / 2 - 13,
            shot: None,
            bomb: None,
            score: 0,
            over: false,
            tick: 0,
            rng: seed | 1,
        }
    }
    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }
    fn abox(&self, r: usize, c: usize) -> (isize, isize, isize, isize) {
        let (_, w) = sprite(r);
        (self.fx + c as isize * PITCH_X, self.fy + r as isize * PITCH_Y, w * SCALE, 8 * SCALE)
    }
    fn left(&self) -> usize { self.alive.iter().flatten().filter(|a| **a).count() }

    fn step(&mut self, dx: isize, fire: bool) -> bool {
        if self.over {
            return false;
        }
        let mut killed = false;
        self.tick = self.tick.wrapping_add(1);

        self.player = (self.player + dx * 4).max(2).min(W - 13 * SCALE - 2);
        if fire && self.shot.is_none() {
            self.shot = Some((self.player + 13 * SCALE / 2, H - 22));
        }

        // fleet marches faster as it thins
        let gone = COLS * ROWS - self.left();
        let period = (7u32).saturating_sub((gone / 3) as u32).max(2);
        if self.tick % period == 0 {
            let (mut lo, mut hi) = (W, 0);
            for r in 0..ROWS {
                for c in 0..COLS {
                    if self.alive[r][c] {
                        let (x, _, w, _) = self.abox(r, c);
                        lo = lo.min(x);
                        hi = hi.max(x + w);
                    }
                }
            }
            if (self.dir > 0 && hi >= W - 2) || (self.dir < 0 && lo <= 2) {
                self.dir = -self.dir;
                self.fy += 5;
            } else {
                self.fx += self.dir * 3;
            }
        }

        if let Some((sx, sy)) = self.shot {
            let ny = sy - 6;
            self.shot = if ny < 0 { None } else { Some((sx, ny)) };
            if let Some((sx, sy)) = self.shot {
                'hit: for r in 0..ROWS {
                    for c in 0..COLS {
                        if !self.alive[r][c] {
                            continue;
                        }
                        let (x, y, w, h) = self.abox(r, c);
                        if sx >= x && sx < x + w && sy >= y && sy < y + h {
                            self.alive[r][c] = false;
                            self.shot = None;
                            self.score += match r { 0 => 30, 1 => 20, _ => 10 };
                            killed = true;
                            break 'hit;
                        }
                    }
                }
            }
        }

        if self.bomb.is_none() && self.rand() % 20 == 0 {
            let mut live = Vec::new();
            for r in 0..ROWS {
                for c in 0..COLS {
                    if self.alive[r][c] {
                        live.push((r, c));
                    }
                }
            }
            if !live.is_empty() {
                let (r, c) = live[(self.rand() as usize) % live.len()];
                let (x, y, w, h) = self.abox(r, c);
                self.bomb = Some((x + w / 2, y + h));
            }
        }
        if let Some((bx, by)) = self.bomb {
            let ny = by + 4;
            self.bomb = if ny >= H { None } else { Some((bx, ny)) };
            if let Some((bx, by)) = self.bomb {
                if by >= H - 18 && bx >= self.player && bx < self.player + 13 * SCALE {
                    self.over = true;
                }
            }
        }
        if self.left() == 0 || self.fy + (ROWS as isize) * PITCH_Y >= H - 22 {
            self.over = true;
        }
        killed
    }

    fn render(&self, fb: &mut [u32; 512]) {
        fb.iter_mut().for_each(|w| *w = 0xFFFF_FFFF); // all dark
        for r in 0..ROWS {
            let (rows, w) = sprite(r);
            for c in 0..COLS {
                if self.alive[r][c] {
                    let (x, y, _, _) = self.abox(r, c);
                    blit(fb, rows, w, x, y);
                }
            }
        }
        blit(fb, &CANNON, 13, self.player, H - 20);
        if let Some((sx, sy)) = self.shot {
            for i in 0..6 { px(fb, sx, sy + i, true); px(fb, sx + 1, sy + i, true); }
        }
        if let Some((bx, by)) = self.bomb {
            for i in 0..6 { px(fb, bx, by + i, true); px(fb, bx + 1, by + i, true); }
        }
        for x in 0..W { px(fb, x, H - 1, true); }
    }
}

impl<'a> ShellCmdApi<'a> for Invaders {
    cmd_api!(invaders);

    fn process(&mut self, _args: String, _env: &mut CommonEnv) -> Result<Option<String>, xous::Error> {
        use core::fmt::Write;

        use ux_api::service::gfx::Gfx;

        let mut ret = String::new();
        let xns = xous_names::XousNames::new().unwrap();
        let gfx = match Gfx::new(&xns) {
            Ok(g) => g,
            Err(_) => { write!(ret, "no gfx").ok(); return Ok(Some(ret)); }
        };
        let tt = ticktimer::Ticktimer::new().unwrap();
        gfx.brightness(BRIGHT).ok(); // the panel is invisible without this

        // ---- input thread: get_keys_blocking() blocks, so it lives off the game loop
        let dx = Arc::new(AtomicI32::new(0));
        let fire = Arc::new(AtomicBool::new(false));
        let quit = Arc::new(AtomicBool::new(false));
        {
            let (dx, fire, quit) = (dx.clone(), fire.clone(), quit.clone());
            std::thread::spawn(move || {
                let xns = xous_names::XousNames::new().unwrap();
                let kbd = match bao1x_api::keyboard::Keyboard::new(&xns) {
                    Ok(k) => k,
                    Err(_) => return,
                };
                while !quit.load(Ordering::Relaxed) {
                    for c in kbd.get_keys_blocking() {
                        match c {
                            '\u{2190}' => dx.store(-1, Ordering::Relaxed), // left
                            '\u{2192}' => dx.store(1, Ordering::Relaxed),  // right
                            '\u{1F525}' => fire.store(true, Ordering::Relaxed), // center
                            '\u{2234}' => quit.store(true, Ordering::Relaxed),  // select
                            _ => {}
                        }
                    }
                }
            });
        }

        let mut fb = [0xFFFF_FFFFu32; 512];
        let mut seed = _env.trng.get_u32().unwrap_or(0x1234_5678);
        let mut g = Game::new(seed);
        // Attract mode until a button is touched, then the human is driving.
        let mut demo = true;

        // Pace off the ticktimer's own clock. sleep_ms() alone did not delay
        // (3000 frames burned in ~30s), so hold a target deadline per frame and
        // sleep until it passes; also cap on WALL TIME rather than frame count.
        const FRAME_MS: u64 = 100;
        const RUN_MS: u64 = 120_000;
        let t0 = tt.elapsed_ms();
        let mut frames = 0u32;
        let mut next = t0 + FRAME_MS;
        let reason;
        loop {
            if g.over {
                if demo {
                    // Attract mode never ends on its own: reseed and play again.
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    g = Game::new(seed);
                    gfx.brightness(BRIGHT).ok(); // survive a reboot resetting it
                    continue;
                }
                reason = "destroyed";
                break;
            }
            if quit.load(Ordering::Relaxed) { reason = "quit"; break; }
            let now = tt.elapsed_ms();
            if now - t0 > RUN_MS { reason = "timeout"; break; }

            let hd = dx.swap(0, Ordering::Relaxed) as isize;
            let hf = fire.swap(false, Ordering::Relaxed);
            if hd != 0 || hf {
                demo = false; // a human touched a button - stop autopiloting
            }
            let (d, f) = if demo { autopilot(&g) } else { (hd, hf) };
            let _killed = g.step(d, f);
            g.render(&mut fb);
            gfx.bitmap(&fb, None, None).ok();
            gfx.flush().ok();
            frames += 1;
            if frames % 20 == 0 {
                gfx.brightness(BRIGHT).ok();
            }

            // spin-with-yield until the frame deadline; robust even if
            // sleep_ms is a no-op on this build
            loop {
                let n = tt.elapsed_ms();
                if n >= next { break; }
                let remain = next - n;
                if remain > 4 { tt.sleep_ms((remain - 2) as usize).ok(); } else { xous::yield_slice(); }
            }
            next += FRAME_MS;
        }
        quit.store(true, Ordering::Relaxed);
        let secs = (tt.elapsed_ms() - t0) as f32 / 1000.0;
        write!(ret, "game over ({}) - score {} - {} frames in {:.1}s", reason, g.score, frames, secs).ok();
        Ok(Some(ret))
    }
}

/// Full badge app: attract-mode game, menu, LED patterns, sleep.
///
/// BUTTONS
///   Select ('\u{2234}') .. open / close the menu   <- the side toggle
///   Up / Down ............ move the menu cursor
///   Center / fire ........ choose, or shoot in-game
///   Left / Right ......... move ship in-game
///
/// Any button press takes the game off autopilot; 20s idle hands it back.
pub fn run_attract() -> ! {
    use crate::cmds::menu::{self, GameKind, Item, LedMode};
    use ux_api::service::gfx::Gfx;

    let xns = xous_names::XousNames::new().unwrap();
    let tt = ticktimer::Ticktimer::new().unwrap();
    let gfx = loop {
        if let Ok(g) = Gfx::new(&xns) { break g; }
        tt.sleep_ms(500).ok();
    };

    // boot1 arms a 60s reset watchdog on battery and nothing in xous-core feeds
    // it on bao1x -- that is the mystery reboot. Feed it every frame.
    let mut wdt = bao1x_hal::wdt::Wdt::new();

    // one driver for the whole run -- see menu::led_open
    let mut leds = menu::led_open();

    // ---- input thread: get_keys_blocking() blocks, so it lives off the loop
    let dx = Arc::new(AtomicI32::new(0));
    let fire = Arc::new(AtomicBool::new(false));
    let menu_key = Arc::new(AtomicBool::new(false));
    let updown = Arc::new(AtomicI32::new(0));
    {
        let (dx, fire, menu_key, updown) =
            (dx.clone(), fire.clone(), menu_key.clone(), updown.clone());
        std::thread::spawn(move || {
            let xns = xous_names::XousNames::new().unwrap();
            if let Ok(kbd) = bao1x_api::keyboard::Keyboard::new(&xns) {
                loop {
                    for c in kbd.get_keys_blocking() {
                        match c {
                            '\u{2190}' => dx.store(-1, Ordering::Relaxed),
                            '\u{2192}' => dx.store(1, Ordering::Relaxed),
                            '\u{1F525}' => fire.store(true, Ordering::Relaxed),
                            '\u{2191}' => updown.store(-1, Ordering::Relaxed),
                            '\u{2193}' => updown.store(1, Ordering::Relaxed),
                            '\u{2234}' => menu_key.store(true, Ordering::Relaxed),
                            _ => {}
                        }
                    }
                }
            }
        });
    }

    let mut fb = [0xFFFF_FFFFu32; 512];
    let mut seed = 0xC0FF_EE01u32;
    let mut g = Game::new(seed);
    let mut airsea = crate::cmds::airsea::AirSea::new(seed ^ 0x5AA5_1234);

    let mut in_menu = false;
    let mut sleeping = false;
    let mut sel: usize = 0;
    let mut game_kind = GameKind::Invaders;
    let mut led_mode = LedMode::GameReactive;
    let mut bright: u8 = 200;
    let mut demo = true;
    let mut idle_ms: u32 = 0;
    let mut t: u32 = 0;
    let mut flash_ttl: u8 = 0;

    gfx.brightness(bright).ok();

    loop {
        t = t.wrapping_add(1);
        wdt.feed();

        let hd = dx.swap(0, Ordering::Relaxed) as isize;
        let hf = fire.swap(false, Ordering::Relaxed);
        let hm = menu_key.swap(false, Ordering::Relaxed);
        let hu = updown.swap(0, Ordering::Relaxed);
        let any = hd != 0 || hf || hm || hu != 0;

        // ---- sleep: everything dark until a button wakes it ---------------
        if sleeping {
            if any {
                sleeping = false;
                gfx.set_power(true).ok();
                gfx.brightness(bright).ok();
            } else {
                menu::led_push(&mut leds, &[(0, 0, 0); menu::LED_N]);
                tt.sleep_ms(150).ok();
                continue;
            }
        }

        if hm {
            in_menu = !in_menu;
            gfx.brightness(bright).ok();
        }

        if in_menu {
            // ---- menu navigation ------------------------------------------
            if hu != 0 {
                let n = menu::ITEMS.len() as i32;
                sel = (((sel as i32 + hu) % n + n) % n) as usize;
            }
            if hf {
                match menu::ITEMS[sel] {
                    Item::Resume => in_menu = false,
                    Item::Game => {
                        game_kind = game_kind.next();
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        g = Game::new(seed);
                        demo = true;
                    }
                    Item::Leds => led_mode = led_mode.next(),
                    Item::Bright => {
                        bright = match bright { 0..=79 => 120, 80..=159 => 200, 160..=239 => 255, _ => 40 };
                        gfx.brightness(bright).ok();
                    }
                    Item::Sleep => {
                        sleeping = true;
                        in_menu = false;
                        menu::led_push(&mut leds, &[(0, 0, 0); menu::LED_N]);
                        gfx.set_power(false).ok();
                        tt.sleep_ms(300).ok();
                        continue;
                    }
                }
            }
            menu::draw_menu(&mut fb, sel, game_kind, led_mode, bright);
        } else {
            // ---- game ------------------------------------------------------
            if hd != 0 || hf { demo = false; idle_ms = 0; }
            else if !demo {
                idle_ms = idle_ms.saturating_add(100);
                if idle_ms > 20_000 { demo = true; }
            }
            match game_kind {
                GameKind::Invaders => {
                    let (d, f) = if demo { autopilot(&g) } else { (hd, hf) };
                    if g.step(d, f) { flash_ttl = 3; }
                    if g.over {
                        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                        g = Game::new(seed);
                        demo = true;
                        gfx.brightness(bright).ok();
                    }
                    g.render(&mut fb);
                }
                GameKind::AirSea => {
                    // simple autopilot: track the lowest live target and fire
                    let (d, f) = if demo {
                        let mut tx = None;
                        let mut best = -1isize;
                        for t in airsea.targets.iter() {
                            if t.alive && t.y > best { best = t.y; tx = Some(t.x + 5); }
                        }
                        let muzzle = airsea.gun_x + 5;
                        match tx {
                            Some(x) => {
                                let d = if x > muzzle + 3 { 1 } else if x < muzzle - 3 { -1 } else { 0 };
                                (d, d == 0)
                            }
                            None => (0, false),
                        }
                    } else { (hd, hf) };
                    if airsea.step(d, f) { flash_ttl = 3; }
                    airsea.render(&mut fb);
                }
            }
        }

        gfx.bitmap(&fb, None, None).ok();
        gfx.flush().ok();

        // ---- LEDs --------------------------------------------------------
        let strip = menu::led_frame(led_mode, t, flash_ttl > 0);
        menu::led_push(&mut leds, &strip);
        flash_ttl = flash_ttl.saturating_sub(1);

        // Re-assert brightness periodically: it starts at zero, so any reset
        // would otherwise leave a black screen forever.
        if t % 50 == 0 { gfx.brightness(bright).ok(); }

        tt.sleep_ms(100).ok();
    }
}
