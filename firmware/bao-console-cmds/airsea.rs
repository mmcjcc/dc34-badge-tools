//! Air-Sea Battle: a genuinely separate game from Invaders.
//!
//! You are a gun at the bottom centre. Planes cross the sky at three altitudes
//! (fast, worth more the higher they are) and boats cross the waterline at the
//! bottom (slow, worth less). Shoot upward; one shell in flight at a time.
//!
//! LED layout note (dc34-console/src/bio/lightgenes/main.c):
//!     index 0,1 = the "eyes"   (on the BACK of the badge)
//!     index 2..9 = the ring    (on the FRONT)

use crate::cmds::menu::{H, W, px};

const GUN_W: isize = 11;
const MAX_TARGETS: usize = 6;

#[derive(Clone, Copy)]
pub struct Target {
    pub x: isize,
    pub y: isize,
    pub vx: isize,
    pub plane: bool,
    pub alive: bool,
}

pub struct AirSea {
    pub gun_x: isize,
    pub shot: Option<(isize, isize)>,
    pub targets: [Target; MAX_TARGETS],
    pub score: u32,
    pub misses: u32,
    pub over: bool,
    rng: u32,
    tick: u32,
}

// 11x5 plane, 11x4 boat -- wide and low so they read at this size
const PLANE: [u16; 5] = [
    0b00000100000,
    0b00001110000,
    0b11111111111,
    0b00111111100,
    0b00000100000,
];
const BOAT: [u16; 4] = [
    0b00000100000,
    0b00001110000,
    0b01111111110,
    0b00111111100,
];

impl AirSea {
    pub fn new(seed: u32) -> Self {
        let mut s = AirSea {
            gun_x: W / 2 - GUN_W / 2,
            shot: None,
            targets: [Target { x: 0, y: 0, vx: 0, plane: true, alive: false }; MAX_TARGETS],
            score: 0,
            misses: 0,
            over: false,
            rng: seed | 1,
            tick: 0,
        };
        for i in 0..3 {
            s.spawn(i);
        }
        s
    }

    fn rand(&mut self) -> u32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng
    }

    fn spawn(&mut self, i: usize) {
        let r = self.rand();
        let plane = (r & 1) == 0 || i < 2; // bias toward planes
        let lane = (r >> 4) % 3;
        let y = if plane { 14 + lane as isize * 16 } else { H - 26 };
        let dir = if (r >> 8) & 1 == 0 { 1 } else { -1 };
        let speed = if plane { 2 + ((r >> 12) % 3) as isize } else { 1 };
        self.targets[i] = Target {
            x: if dir > 0 { -12 } else { W + 12 },
            y,
            vx: dir * speed,
            plane,
            alive: true,
        };
    }

    /// Returns true if something was hit this frame (for the LED flash).
    pub fn step(&mut self, dx: isize, fire: bool) -> bool {
        if self.over {
            return false;
        }
        self.tick = self.tick.wrapping_add(1);
        let mut hit = false;

        self.gun_x = (self.gun_x + dx * 4).max(2).min(W - GUN_W - 2);
        if fire && self.shot.is_none() {
            self.shot = Some((self.gun_x + GUN_W / 2, H - 18));
        }

        // targets drift; respawn when they leave the screen
        for i in 0..MAX_TARGETS {
            if !self.targets[i].alive {
                if self.rand() % 40 == 0 {
                    self.spawn(i);
                }
                continue;
            }
            self.targets[i].x += self.targets[i].vx;
            if self.targets[i].x < -16 || self.targets[i].x > W + 16 {
                self.targets[i].alive = false;
            }
        }

        // shell travels up
        if let Some((sx, sy)) = self.shot {
            let ny = sy - 5;
            if ny < 0 {
                self.shot = None;
                self.misses += 1;
            } else {
                self.shot = Some((sx, ny));
                'hit: for i in 0..MAX_TARGETS {
                    let t = self.targets[i];
                    if !t.alive {
                        continue;
                    }
                    let (tw, th) = if t.plane { (11, 5) } else { (11, 4) };
                    if sx >= t.x && sx < t.x + tw && ny >= t.y && ny < t.y + th {
                        self.targets[i].alive = false;
                        self.shot = None;
                        // higher planes are worth more; boats are the easy points
                        self.score += if t.plane { 30 - (t.y as u32 / 8) } else { 10 };
                        hit = true;
                        break 'hit;
                    }
                }
            }
        }
        hit
    }

    pub fn render(&self, fb: &mut [u32; 512]) {
        crate::cmds::menu::clear(fb);

        // waterline
        for x in 0..W {
            px(fb, x, H - 20, true);
        }

        for t in self.targets.iter() {
            if !t.alive {
                continue;
            }
            let (rows, n): (&[u16], usize) =
                if t.plane { (&PLANE, 5) } else { (&BOAT, 4) };
            for ry in 0..n {
                for rx in 0..11 {
                    if (rows[ry] >> (10 - rx)) & 1 == 1 {
                        px(fb, t.x + rx as isize, t.y + ry as isize, true);
                    }
                }
            }
        }

        // gun: a squat turret
        for x in 0..GUN_W {
            for y in 0..4 {
                px(fb, self.gun_x + x, H - 8 + y, true);
            }
        }
        for y in 0..4 {
            px(fb, self.gun_x + GUN_W / 2, H - 12 + y, true);
            px(fb, self.gun_x + GUN_W / 2 + 1, H - 12 + y, true);
        }

        if let Some((sx, sy)) = self.shot {
            for i in 0..4 {
                px(fb, sx, sy + i, true);
            }
        }

        crate::cmds::menu::text(fb, "AIR-SEA", 2, 2, 1);
    }
}
