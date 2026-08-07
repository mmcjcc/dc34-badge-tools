use String;

use crate::{CommonEnv, ShellCmdApi};

/// LED control for the DC34 badge's on-board RGB ring.
///
/// WHY THE FIRST SCAN FOUND NOTHING: sweeping BIO bit indices 0..31 and calling
/// Ws2812::new() is not enough. `Ws2812::new` only sets the PIOSEL mux bit; it
/// never sets the pin's ALTERNATE FUNCTION. Look at what the real mapping call
/// does (bao1x-hal/src/iox.rs):
///
///     self.set_alternate_function(port, pin, IoxFunction::AF1);   // <-- required
///     self.csr.wo(iox::SFR_PIOSEL, ... | (1 << bio_bit));
///
/// Without AF1 the pad is not electrically connected to BIO at all, so the core
/// happily toggles a bit that reaches no pin. `led scan` therefore now iterates
/// PHYSICAL port/pin pairs and calls set_bio_bit_from_port_and_pin(), which
/// returns the BIO bit to hand to Ws2812.
///
/// Only PB and PC map to BIO:
///     PB pin 0..15 -> bio bit 15 - pin
///     PC pin 0..15 -> bio bit pin + 16
/// everything else returns None.
///
/// Also: never call Ws2812::send() -- it spins forever on a completion token
/// with no timeout. send_async() only.
#[derive(Debug)]
pub struct Led {}

#[cfg(feature = "bio-lib")]
const N_LEDS: usize = 10;

#[cfg(feature = "bio-lib")]
fn drive(bio_bit: u8, r: u8, g: u8, b: u8, on_ms: usize, tt: &ticktimer::Ticktimer) {
    use arbitrary_int::u5;
    use bio_lib::ws2812::{LedVariant, Ws2812, rgb_to_u32};
    if let Ok(mut ws) = Ws2812::new(LedVariant::C, u5::new(bio_bit & 0x1f), None) {
        let px = rgb_to_u32(r, g, b);
        ws.send_async(&[px; N_LEDS]);
        tt.sleep_ms(on_ms).ok();
        ws.send_async(&[0u32; N_LEDS]);
        tt.sleep_ms(60).ok();
    }
}

impl<'a> ShellCmdApi<'a> for Led {
    cmd_api!(led);

    fn process(&mut self, args: String, _env: &mut CommonEnv) -> Result<Option<String>, xous::Error> {
        use core::fmt::Write;
        let mut ret = String::new();
        let help = "Usage:
  led scan            - map each PB/PC pin to BIO and flash it (watch the ring)
  led pb <pin> <r> <g> <b>
  led pc <pin> <r> <g> <b>
  led off <pb|pc> <pin>";

        #[cfg(not(feature = "bio-lib"))]
        { let _ = args; write!(ret, "bio-lib not compiled in").ok(); return Ok(Some(ret)); }

        #[cfg(feature = "bio-lib")]
        {
            use bao1x_api::iox::{IoSetup, IoxHal, IoxPort};
            let tt = ticktimer::Ticktimer::new().unwrap();
            let iox = IoxHal::new();

            let mut tok = args.split_whitespace();
            let port_of = |s: &str| match s {
                "pb" | "PB" => Some(IoxPort::PB),
                "pc" | "PC" => Some(IoxPort::PC),
                _ => None,
            };

            match tok.next() {
                Some("scan") => {
                    write!(ret, "scanning PB0-15 then PC0-15; watch the badge\n").ok();
                    for (pname, port) in [("PB", IoxPort::PB), ("PC", IoxPort::PC)] {
                        for pin in 0u8..16 {
                            // Map the PHYSICAL pin onto a BIO bit (sets AF1 + PIOSEL).
                            match iox.set_bio_bit_from_port_and_pin(port, pin) {
                                Some(bit) => {
                                    log::info!("LED scan: {}{} -> bio bit {}", pname, pin, bit);
                                    drive(bit, 0, 120, 0, 200, &tt);
                                }
                                None => log::info!("LED scan: {}{} not BIO-mappable", pname, pin),
                            }
                        }
                    }
                    write!(ret, "scan done - note which pin was logged when it lit").ok();
                }
                Some(p) if port_of(p).is_some() => {
                    let port = port_of(p).unwrap();
                    let pin: u8 = tok.next().unwrap_or("0").parse().unwrap_or(0);
                    let r: u8 = tok.next().unwrap_or("0").parse().unwrap_or(0);
                    let g: u8 = tok.next().unwrap_or("0").parse().unwrap_or(0);
                    let b: u8 = tok.next().unwrap_or("0").parse().unwrap_or(0);
                    match iox.set_bio_bit_from_port_and_pin(port, pin) {
                        Some(bit) => {
                            drive(bit, r, g, b, 1500, &tt);
                            write!(ret, "{} pin {} -> bio {} : r{} g{} b{}", p, pin, bit, r, g, b).ok();
                        }
                        None => { write!(ret, "{} pin {} is not BIO-mappable", p, pin).ok(); }
                    }
                }
                _ => { write!(ret, "{}", help).ok(); }
            };
            Ok(Some(ret))
        }
    }
}
