//! invaders.rs — Space Invaders for the DEF CON 34 badge.
//!
//! Requires custom firmware (developer mode). The stock console can only push
//! whole static framebuffers over serial (~1-3 s each, and every one is a write
//! to the PDDB), so an interactive game is not possible without owning the app.
//!
//! WHY THIS GAME FITS THIS HARDWARE
//!   * 1978 Space Invaders is natively 1-bit monochrome, so a 128x128 mono OLED
//!     is the *correct* medium — no dithering or greyscale faking, unlike a
//!     console emulator which would look bad here.
//!   * It tolerates a variable frame rate. That matters: the badge's display SPI
//!     link intermittently times out and resets itself ("timeout in draw" ->
//!     "resetting display spim block"), stalling redraws for up to seconds.
//!     A twitch game would feel broken; this degrades gracefully.
//!   * It needs exactly three inputs, and the badge has Left / Right / Center.
//!
//! POLARITY (verified on hardware): `clear()` fills 0xFFFFFFFF and a set bit
//! renders DARK, so a cleared screen is black and sprites must be drawn with
//! ColorNative(0) to glow. This is inverted from intuition — see `LIT`/`DARK`.
//!
//! The sprite art and layout in this file were validated on the real panel
//! before any of it was written, by rendering frames on the host and pushing
//! them over the serial `image` command.

use ux_api::minigfx::{ColorNative, FrameBuffer, Point};

pub const W: isize = 128;
pub const H: isize = 128;

/// A set bit renders dark on this panel, so "lit" is 0. Verified on hardware.
fn lit() -> ColorNative { 0u8.into() }
fn dark() -> ColorNative { 1u8.into() }

// --- sprite art: 1 bit per pixel, MSB-left, one u16 row per line -----------
const SQUID: [u16; 8] =
    [0b00011000, 0b00111100, 0b01111110, 0b11011011, 0b11111111, 0b00100100, 0b01011010, 0b10100101];
const CRAB: [u16; 8] = [
    0b00100000100, 0b00010001000, 0b00111111100, 0b01101110110,
    0b11111111111, 0b10111111101, 0b10100000101, 0b00011011000,
];
const OCTOPUS: [u16; 8] = [
    0b00011111000, 0b01111111110, 0b11111111111, 0b11001110011,
    0b11111111111, 0b00110101100, 0b01100100110, 0b11000000011,
];
const CANNON: [u16; 8] = [
    0b0000001000000, 0b0000011100000, 0b0000011100000, 0b0111111111110,
    0b1111111111111, 0b1111111111111, 0b1111111111111, 0b1111111111111,
];

struct Sprite {
    rows: &'static [u16; 8],
    w: isize,
}
const S_SQUID: Sprite = Sprite { rows: &SQUID, w: 8 };
const S_CRAB: Sprite = Sprite { rows: &CRAB, w: 11 };
const S_OCTO: Sprite = Sprite { rows: &OCTOPUS, w: 11 };
const S_CANNON: Sprite = Sprite { rows: &CANNON, w: 13 };

const COLS: usize = 5;
const ROWS: usize = 3;
const PITCH_X: isize = 23;
const PITCH_Y: isize = 18;
const SCALE: isize = 2;

pub struct Game {
    alive: [[bool; COLS]; ROWS],
    fleet_x: isize,
    fleet_y: isize,
    dir: isize,
    player_x: isize,
    shot: Option<(isize, isize)>,
    bomb: Option<(isize, isize)>,
    pub score: u32,
    pub over: bool,
    /// Set for one frame when an alien dies — the caller flashes the LEDs.
    pub killed_this_frame: bool,
    tick: u32,
    rng: u32,
}

impl Game {
    pub fn new(seed: u32) -> Self {
        Game {
            alive: [[true; COLS]; ROWS],
            fleet_x: 6,
            fleet_y: 30,
            dir: 1,
            player_x: W / 2 - 13,
            shot: None,
            bomb: None,
            score: 0,
            over: false,
            killed_this_frame: false,
            tick: 0,
            rng: seed | 1,
        }
    }

    fn rand(&mut self) -> u32 {
        // xorshift32 — the SCE TRNG is overkill for picking which alien shoots
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn sprite_for(row: usize) -> &'static Sprite {
        match row {
            0 => &S_SQUID,
            1 => &S_CRAB,
            _ => &S_OCTO,
        }
    }

    fn alien_box(&self, r: usize, c: usize) -> (isize, isize, isize, isize) {
        let sp = Self::sprite_for(r);
        let x = self.fleet_x + c as isize * PITCH_X;
        let y = self.fleet_y + r as isize * PITCH_Y;
        (x, y, sp.w * SCALE, 8 * SCALE)
    }

    fn remaining(&self) -> usize {
        self.alive.iter().flatten().filter(|a| **a).count()
    }

    pub fn update(&mut self, left: bool, right: bool, fire: bool) {
        if self.over {
            return;
        }
        self.tick = self.tick.wrapping_add(1);
        self.killed_this_frame = false;

        // player
        if left {
            self.player_x = (self.player_x - 3).max(2);
        }
        if right {
            self.player_x = (self.player_x + 3).min(W - S_CANNON.w * SCALE - 2);
        }
        if fire && self.shot.is_none() {
            self.shot = Some((self.player_x + S_CANNON.w * SCALE / 2, H - 20));
        }

        // fleet marches faster as it thins out — the original's rising panic
        let speed = 1 + (COLS * ROWS - self.remaining()) / 4;
        if self.tick % (12u32.saturating_sub(speed as u32).max(2)) == 0 {
            let mut lo = W;
            let mut hi = 0;
            for r in 0..ROWS {
                for c in 0..COLS {
                    if self.alive[r][c] {
                        let (x, _, w, _) = self.alien_box(r, c);
                        lo = lo.min(x);
                        hi = hi.max(x + w);
                    }
                }
            }
            if (self.dir > 0 && hi >= W - 2) || (self.dir < 0 && lo <= 2) {
                self.dir = -self.dir;
                self.fleet_y += 6;
            } else {
                self.fleet_x += self.dir * 2;
            }
        }

        // player shot
        if let Some((sx, sy)) = self.shot {
            let ny = sy - 5;
            self.shot = if ny < 0 { None } else { Some((sx, ny)) };
            if let Some((sx, sy)) = self.shot {
                'hit: for r in 0..ROWS {
                    for c in 0..COLS {
                        if !self.alive[r][c] {
                            continue;
                        }
                        let (x, y, w, h) = self.alien_box(r, c);
                        if sx >= x && sx < x + w && sy >= y && sy < y + h {
                            self.alive[r][c] = false;
                            self.shot = None;
                            self.score += match r {
                                0 => 30,
                                1 => 20,
                                _ => 10,
                            };
                            self.killed_this_frame = true;
                            break 'hit;
                        }
                    }
                }
            }
        }

        // alien bomb
        if self.bomb.is_none() && self.rand() % 24 == 0 {
            let live: heapless::Vec<(usize, usize), { ROWS * COLS }> = (0..ROWS)
                .flat_map(|r| (0..COLS).map(move |c| (r, c)))
                .filter(|&(r, c)| self.alive[r][c])
                .collect();
            if !live.is_empty() {
                let (r, c) = live[(self.rand() as usize) % live.len()];
                let (x, y, w, h) = self.alien_box(r, c);
                self.bomb = Some((x + w / 2, y + h));
            }
        }
        if let Some((bx, by)) = self.bomb {
            let ny = by + 3;
            self.bomb = if ny >= H { None } else { Some((bx, ny)) };
            if let Some((bx, by)) = self.bomb {
                if by >= H - 18
                    && bx >= self.player_x
                    && bx < self.player_x + S_CANNON.w * SCALE
                {
                    self.over = true;
                }
            }
        }

        if self.remaining() == 0 || self.fleet_y + (ROWS as isize) * PITCH_Y >= H - 20 {
            self.over = true;
        }
    }

    pub fn render<F: FrameBuffer>(&self, fb: &mut F) {
        fb.clear(); // cleared == dark on this panel
        let on = lit();

        for r in 0..ROWS {
            let sp = Self::sprite_for(r);
            for c in 0..COLS {
                if !self.alive[r][c] {
                    continue;
                }
                let (x, y, _, _) = self.alien_box(r, c);
                blit(fb, sp, x, y, on);
            }
        }
        blit(fb, &S_CANNON, self.player_x, H - 18, on);

        if let Some((sx, sy)) = self.shot {
            vline(fb, sx, sy, 5, on);
        }
        if let Some((bx, by)) = self.bomb {
            vline(fb, bx, by, 5, on);
        }
        for x in 0..W {
            fb.put_pixel(Point::new(x, H - 1), on);
        }
        let _ = fb.draw(); // ignore SPI resets; next frame repaints anyway
    }
}

fn blit<F: FrameBuffer>(fb: &mut F, sp: &Sprite, x: isize, y: isize, c: ColorNative) {
    for (ry, row) in sp.rows.iter().enumerate() {
        for rx in 0..sp.w {
            if (row >> (sp.w - 1 - rx)) & 1 == 0 {
                continue;
            }
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    let px = x + rx * SCALE + dx;
                    let py = y + ry as isize * SCALE + dy;
                    if px >= 0 && px < W && py >= 0 && py < H {
                        fb.put_pixel(Point::new(px, py), c);
                    }
                }
            }
        }
    }
}

fn vline<F: FrameBuffer>(fb: &mut F, x: isize, y: isize, len: isize, c: ColorNative) {
    for i in 0..len {
        let py = y + i;
        if py >= 0 && py < H {
            fb.put_pixel(Point::new(x, py), c);
            fb.put_pixel(Point::new(x + 1, py), c);
        }
    }
}

#[allow(dead_code)]
fn unused(_: ColorNative) { let _ = dark(); }
