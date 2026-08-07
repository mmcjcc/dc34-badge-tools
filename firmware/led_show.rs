//! led_show.rs — light shows for the DEF CON 34 badge's on-board RGB LEDs.
//!
//! Requires custom firmware (developer mode). The stock build gives you NO
//! access to these: every LED verb (`hue`, `autogamy`, `transmute`, `mate`,
//! `rate`) is compiled out of the shipped console, and the light engine's BIO
//! FIFO is not reachable from the `bio` command either.
//!
//! The driver itself is public — `libs/bio-lib/src/ws2812.rs` in
//! betrusted-io/xous-core. All the sub-microsecond WS2812 bit timing runs on a
//! BIO coprocessor core at a 150 ns quantum (6.667 MHz), so the application CPU
//! only ever pushes 24-bit words into a FIFO. That is why this file is short.
//!
//! WIRE FORMAT (from the ws2812b/ws2812c BIO kernels):
//!   * The FIRST word ever written is the GPIO pin mask, not a colour.
//!     `Ws2812::new()` does that for you.
//!   * Colours are packed GRB, not RGB: `rgb_to_u32(r,g,b)`.
//!   * Data is sent LAST-LED-FIRST. `send()` takes the strip in chain order and
//!     sets the commit bit (bit 24) on the final word.
//!
//! PIN: the reference app (`apps-dabao/dabao-console/src/cmds/ws2812.rs`) uses
//! `LED_BIO_PIN = 5`, but that is the *dabao* dev board. The badge's pin is not
//! in the public tree, so `scan_for_led_pin()` below finds it empirically —
//! the same approach that cracked the framebuffer encoding.

use bio_lib::ws2812::{LedVariant, Ws2812, rgb_to_u32};

/// Badge LED count: 2 "eyes" + an 8-LED ring. Verify on hardware — if the tail
/// of the ring stays dark, raise this; if the show wraps early, lower it.
pub const N_LEDS: usize = 10;

/// Keep this low. These are bright, they sit right under your chin on a
/// lanyard, and the badge runs from two AA cells.
pub const BRIGHTNESS: u8 = 40;

fn scale(c: u8) -> u8 { ((c as u16 * BRIGHTNESS as u16) / 255) as u8 }

fn put(strip: &mut [u32; N_LEDS], i: usize, r: u8, g: u8, b: u8) {
    strip[i] = rgb_to_u32(scale(r), scale(g), scale(b));
}

/// HSV→RGB with hue in 0..=255. Integer only: the VexRiscv has no FPU, and a
/// float sin() per LED per frame would dominate the frame budget.
fn hsv(h: u8, s: u8, v: u8) -> (u8, u8, u8) {
    if s == 0 {
        return (v, v, v);
    }
    let region = h / 43;
    let rem = (h as u16 % 43) * 6;
    let p = ((v as u16 * (255 - s as u16)) / 255) as u8;
    let q = ((v as u16 * (255 - ((s as u16 * rem) / 255))) / 255) as u8;
    let t = ((v as u16 * (255 - ((s as u16 * (255 - rem)) / 255))) / 255) as u8;
    match region {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Rainbow spinning around the ring.
pub fn rainbow(leds: &mut Ws2812, frame: u32) {
    let mut strip = [0u32; N_LEDS];
    for i in 0..N_LEDS {
        let h = (frame.wrapping_add((i as u32) * (256 / N_LEDS as u32)) & 0xFF) as u8;
        let (r, g, b) = hsv(h, 255, 255);
        put(&mut strip, i, r, g, b);
    }
    leds.send(&strip);
}

/// Larson scanner (Knight Rider). Bounces with a fading tail.
pub fn larson(leds: &mut Ws2812, frame: u32) {
    let mut strip = [0u32; N_LEDS];
    let span = (N_LEDS * 2 - 2) as u32;
    let pos = frame % span;
    let head = if pos < N_LEDS as u32 { pos } else { span - pos } as i32;
    for i in 0..N_LEDS {
        let d = (i as i32 - head).abs();
        let v = match d {
            0 => 255u8,
            1 => 80,
            2 => 20,
            _ => 0,
        };
        put(&mut strip, i, v, 0, 0);
    }
    leds.send(&strip);
}

/// All LEDs one colour — the primitive the morse blinker uses.
pub fn solid(leds: &mut Ws2812, r: u8, g: u8, b: u8) {
    let mut strip = [0u32; N_LEDS];
    for i in 0..N_LEDS {
        put(&mut strip, i, r, g, b);
    }
    leds.send(&strip);
}

pub fn off(leds: &mut Ws2812) { solid(leds, 0, 0, 0); }

/// Blink a morse message on the LEDs. This is the thing stock firmware refused
/// us — we could only fake it by flashing the whole OLED white.
///
/// `unit_ms` is one dot. A dash is 3 units, the gap between elements 1 unit,
/// and between letters 3 units.
pub fn morse(leds: &mut Ws2812, msg: &str, unit_ms: u32, sleep_ms: &dyn Fn(u32)) {
    for ch in msg.chars() {
        let code = match ch.to_ascii_uppercase() {
            'A' => ".-", 'B' => "-...", 'C' => "-.-.", 'D' => "-..",
            'E' => ".", 'F' => "..-.", 'G' => "--.", 'H' => "....",
            'I' => "..", 'J' => ".---", 'K' => "-.-", 'L' => ".-..",
            'M' => "--", 'N' => "-.", 'O' => "---", 'P' => ".--.",
            'Q' => "--.-", 'R' => ".-.", 'S' => "...", 'T' => "-",
            'U' => "..-", 'V' => "...-", 'W' => ".--", 'X' => "-..-",
            'Y' => "-.--", 'Z' => "--..", ' ' => " ",
            _ => "",
        };
        if code == " " {
            sleep_ms(unit_ms * 7);
            continue;
        }
        for sym in code.chars() {
            solid(leds, 255, 255, 255);
            sleep_ms(if sym == '-' { unit_ms * 3 } else { unit_ms });
            off(leds);
            sleep_ms(unit_ms);
        }
        sleep_ms(unit_ms * 2); // 3 total between letters, 1 already spent
    }
}

/// One-time discovery: the badge's LED pin is not published. Walk the BIO pins
/// flashing bright green on each and watch the badge — whichever pin lights the
/// ring is the answer. Hard-code it afterwards and delete this.
///
/// Skip GPIO4: it is the open-drain wake interrupt and driving it has side
/// effects you do not want mid-scan.
pub fn scan_for_led_pin(sleep_ms: &dyn Fn(u32), log: &dyn Fn(u8)) {
    for pin in 0u8..32 {
        if pin == 4 {
            continue;
        }
        log(pin);
        if let Ok(mut leds) = Ws2812::new(LedVariant::C, arbitrary_int::u5::new(pin & 0x1F), None) {
            for _ in 0..6 {
                solid(&mut leds, 0, 255, 0);
                sleep_ms(120);
                off(&mut leds);
                sleep_ms(120);
            }
        }
        // `leds` drops here, releasing the BIO core and pin for the next try.
        sleep_ms(400);
    }
}
