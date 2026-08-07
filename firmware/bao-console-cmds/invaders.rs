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
        let mut g = Game::new(_env.trng.get_u32().unwrap_or(0x1234_5678));

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
            if g.over { reason = "destroyed"; break; }
            if quit.load(Ordering::Relaxed) { reason = "quit"; break; }
            let now = tt.elapsed_ms();
            if now - t0 > RUN_MS { reason = "timeout"; break; }

            let d = dx.swap(0, Ordering::Relaxed) as isize;
            let f = fire.swap(false, Ordering::Relaxed);
            let _killed = g.step(d, f);
            g.render(&mut fb);
            gfx.bitmap(&fb, None, None).ok();
            gfx.flush().ok();
            frames += 1;

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
