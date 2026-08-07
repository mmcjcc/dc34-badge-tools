use String;

use crate::{CommonEnv, ShellCmdApi};

/// LED control for the DC34 badge.
///
/// THE ANSWER, from the actual stock firmware (github.com/bunnie/dc34-console,
/// src/leds.rs) -- which was public the whole time under the `bunnie` org, not
/// `baochip` or `betrusted-io`:
///
///     Lightgenes::new(arbitrary_int::u5::new(15), LED_COUNT, None)
///     const LED_COUNT: u8 = 10;   // 18 under the `uber` feature
///
/// So: **BIO bit 15**, 10 pixels (index 0,1 = eyes; 2..9 = ring), GRB, part is
/// WS2812C-2020 (hence LedVariant::C).
///
/// WHY EVERY EARLIER ATTEMPT FAILED -- two compounding mistakes:
///
///  1. `set_bio_bit_from_port_and_pin()` computes `bio_bit = 15 - pin` for PB,
///     but the vendor pinout says PB is IDENTITY (PB15 -> bit 15). That helper
///     is buggy for PB, so asking it for PB15 returned bit 0 and asking for bit
///     15 required PB0. No argument could produce AF1-on-PB15 *and* PIOSEL 15.
///
///  2. Calling it at all was harmful. `Ws2812::new()`/`Lightgenes::new()`
///     already do the mux: they set `io_config.mapped = 1 << bio_pin` and call
///     `setup_io_config()`, which performs `set_alternate_function(.., AF1)`
///     plus PIOSEL. And `IoConfigMode::Overwrite` CLEARS every PIOSEL bit not in
///     the mask, so our manual pre-configuration was being wiped.
///
/// The fix is therefore to do less: hand the driver pin 15 and get out of the way.
///
/// Note `board/baosec.rs::setup_pmic_irq()` configures (PB, 15) as an IRQ input
/// -- a leftover from the reference board. Do not call it here; it fights the
/// LED driver for this exact pin.
#[derive(Debug)]
pub struct Led {}

/// Confirmed in dc34-console/src/leds.rs.
#[cfg(feature = "bio-lib")]
const LED_PIN: u8 = 15;
#[cfg(feature = "bio-lib")]
const LED_COUNT: usize = 10;

#[cfg(feature = "bio-lib")]
fn strip(r: u8, g: u8, b: u8) -> [u32; LED_COUNT] {
    use bio_lib::ws2812::rgb_to_u32;
    [rgb_to_u32(r, g, b); LED_COUNT]
}

impl<'a> ShellCmdApi<'a> for Led {
    cmd_api!(led);

    fn process(&mut self, args: String, _env: &mut CommonEnv) -> Result<Option<String>, xous::Error> {
        use core::fmt::Write;
        let mut ret = String::new();
        let help = "Usage:
  led on [r] [g] [b]  - all 10 LEDs (default white)
  led off
  led rainbow         - hue sweep around the ring
  led eyes <r> <g> <b> - just the two eyes (index 0,1)
  led pin <n>         - override the BIO pin (default 15)";

        #[cfg(not(feature = "bio-lib"))]
        { let _ = args; write!(ret, "bio-lib not compiled in").ok(); return Ok(Some(ret)); }

        #[cfg(feature = "bio-lib")]
        {
            use arbitrary_int::u5;
            use bio_lib::ws2812::{LedVariant, Ws2812, rgb_to_u32};
            let tt = ticktimer::Ticktimer::new().unwrap();
            let mut tok = args.split_whitespace();
            let verb = tok.next().unwrap_or("");

            // Allow overriding the pin for experimentation, but default to the
            // value the stock firmware uses.
            let pin = if verb == "pin" {
                tok.next().unwrap_or("15").parse::<u8>().unwrap_or(LED_PIN)
            } else {
                LED_PIN
            };

            let mut ws = match Ws2812::new(LedVariant::C, u5::new(pin & 0x1f), None) {
                Ok(w) => w,
                Err(e) => {
                    write!(ret, "could not claim BIO for pin {}: {:?}", pin, e).ok();
                    return Ok(Some(ret));
                }
            };

            match verb {
                "off" => {
                    ws.send_async(&strip(0, 0, 0));
                    write!(ret, "leds off").ok();
                }
                "rainbow" => {
                    for step in 0u32..256 {
                        let h = (step & 0xff) as u8;
                        let (r, g, b) = match h / 43 {
                            0 => (255, h.wrapping_mul(6), 0),
                            1 => (255u8.wrapping_sub(h.wrapping_mul(6)), 255, 0),
                            2 => (0, 255, h.wrapping_mul(6)),
                            3 => (0, 255u8.wrapping_sub(h.wrapping_mul(6)), 255),
                            4 => (h.wrapping_mul(6), 0, 255),
                            _ => (255, 0, 255u8.wrapping_sub(h.wrapping_mul(6))),
                        };
                        ws.send_async(&strip(r / 4, g / 4, b / 4));
                        tt.sleep_ms(25).ok();
                    }
                    ws.send_async(&strip(0, 0, 0));
                    write!(ret, "rainbow done on pin {}", pin).ok();
                }
                "eyes" => {
                    let r: u8 = tok.next().unwrap_or("255").parse().unwrap_or(255);
                    let g: u8 = tok.next().unwrap_or("0").parse().unwrap_or(0);
                    let b: u8 = tok.next().unwrap_or("0").parse().unwrap_or(0);
                    let mut s = [0u32; LED_COUNT];
                    s[0] = rgb_to_u32(r, g, b);
                    s[1] = rgb_to_u32(r, g, b);
                    ws.send_async(&s);
                    write!(ret, "eyes -> {},{},{}", r, g, b).ok();
                }
                "pin" | "on" | "" => {
                    let r: u8 = tok.next().unwrap_or("120").parse().unwrap_or(120);
                    let g: u8 = tok.next().unwrap_or("120").parse().unwrap_or(120);
                    let b: u8 = tok.next().unwrap_or("120").parse().unwrap_or(120);
                    ws.send_async(&strip(r, g, b));
                    write!(ret, "pin {} -> {},{},{} ({} leds)", pin, r, g, b, LED_COUNT).ok();
                }
                _ => {
                    write!(ret, "{}", help).ok();
                }
            };
            Ok(Some(ret))
        }
    }
}
