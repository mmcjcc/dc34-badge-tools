use String;

use crate::{CommonEnv, ShellCmdApi};

mod frames;

/// Draw to the badge's 128x128 OLED.
///
/// CRITICAL, and the thing that cost us hours: the panel comes up at ZERO
/// BRIGHTNESS. `bao-video` inits it, `gfx.bitmap()` renders correctly, and the
/// server cheerfully reports success -- but you see nothing until you call
/// `gfx.brightness()`. Every draw command below therefore sets brightness
/// first. If the screen is dark, suspect brightness before suspecting the
/// drawing code.
///
/// Second gotcha: `Rectangle::new()` defaults to
/// `DrawStyle { fill_color: None, stroke_color: None, stroke_width: 0 }`, so a
/// shape drawn that way renders NOTHING while still returning Ok. Use
/// `new_with_style`.
#[derive(Debug)]
pub struct Screen {}

const DEFAULT_BRIGHT: u8 = 200;

impl<'a> ShellCmdApi<'a> for Screen {
    cmd_api!(screen);

    fn process(&mut self, args: String, _env: &mut CommonEnv) -> Result<Option<String>, xous::Error> {
        use core::fmt::Write;

        use ux_api::minigfx::*;
        use ux_api::service::gfx::Gfx;

        let mut ret = String::new();
        let help = "Usage:
  screen invaders       - draw the Space Invaders frame
  screen clear          - clear to background
  screen box            - filled test rectangle
  screen bright <0-255> - panel brightness (0 = invisible!)
  screen on / off       - panel power";

        let xns = xous_names::XousNames::new().unwrap();
        let gfx = match Gfx::new(&xns) {
            Ok(g) => g,
            Err(_) => {
                write!(ret, "no graphics server").ok();
                return Ok(Some(ret));
            }
        };

        let mut tokens = args.split_whitespace();
        match tokens.next() {
            Some("invaders") => {
                gfx.brightness(DEFAULT_BRIGHT).ok();
                gfx.bitmap(&frames::INVADERS, None, None).ok();
                gfx.flush().ok();
                write!(ret, "invaders drawn (brightness {})", DEFAULT_BRIGHT).ok();
            }
            Some("clear") => {
                gfx.clear().ok();
                gfx.flush().ok();
                write!(ret, "cleared").ok();
            }
            Some("box") => {
                gfx.brightness(DEFAULT_BRIGHT).ok();
                gfx.clear().ok();
                // Must specify a style: the default draws nothing at all.
                let style = DrawStyle::new(PixelColor::Dark, PixelColor::Dark, 2);
                let r = Rectangle::new_with_style(Point::new(20, 20), Point::new(108, 108), style);
                gfx.draw_rectangle(r).ok();
                gfx.flush().ok();
                write!(ret, "box drawn").ok();
            }
            Some("bright") => {
                let level: u8 = tokens.next().unwrap_or("200").parse().unwrap_or(200);
                gfx.brightness(level).ok();
                write!(ret, "brightness {}", level).ok();
            }
            Some("on") => {
                gfx.set_power(true).ok();
                gfx.brightness(DEFAULT_BRIGHT).ok();
                write!(ret, "panel on").ok();
            }
            Some("off") => {
                gfx.set_power(false).ok();
                write!(ret, "panel off").ok();
            }
            _ => {
                write!(ret, "{}", help).ok();
            }
        }
        Ok(Some(ret))
    }
}
