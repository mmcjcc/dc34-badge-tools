# Running your own firmware on the DC34 badge

## READ THIS FIRST: the badge firmware is PUBLIC, under the `bunnie` org

Not `baochip`, not `betrusted-io` — **`bunnie`**. Searching the first two and
concluding the LED code was proprietary cost us most of a day:

* https://github.com/bunnie/dc34-console — the shell, **and the LED driver**
* https://github.com/bunnie/dc34-api — `LED_SERVER = "_oem_led_"`
* https://github.com/bunnie/dc34-vault — the vault app
* https://github.com/bunnie/dc34-core-hw — core module hardware
* https://github.com/bunnie/dc34-bio — BIO code loader

`dc34-console` pins xous-core to rev `616bf65f6e379165464f50b1e79ec42aff77a683`.
Building its code against HEAD drifts (e.g. `init_core` arity changed).

## The LEDs: pin 15, 10 pixels

Straight from `dc34-console/src/leds.rs`:

```rust
let sid = xns.register_name(dc34_api::LED_SERVER, None).unwrap();
crate::bio::lightgenes::Lightgenes::new(arbitrary_int::u5::new(15), LED_COUNT, None).unwrap();
const LED_COUNT: u8 = 10;   // 18 under the `uber` feature
```

**BIO bit 15. 10 pixels — index 0,1 are the eyes, 2..9 the ring. WS2812C-2020,
GRB, 150 ns quantum (`TargetFreqInt(6_666_667)`).**

### Do NOT call `set_bio_bit_from_port_and_pin()` for this

Two reasons, and together they make brute-force sweeping impossible:

1. **It is buggy for PB.** It computes `bio_bit = 15 - pin`, but the vendor
   pinout (`baochip/dabao/docs/pinout/pins.csv`) says PB is *identity*. So
   asking for PB15 yields bit 0, and bit 15 demands PB0 — **no argument can
   produce AF1-on-PB15 together with PIOSEL bit 15.** A sweep over that helper
   is structurally incapable of lighting these LEDs. PC (`pin + 16`) is fine,
   which is why SAO work behaves normally.
2. **The driver already does the mux.** `Ws2812::new()` / `Lightgenes::new()`
   set `io_config.mapped = 1 << bio_pin` and call `setup_io_config()`, which
   performs `set_alternate_function(.., AF1)` plus PIOSEL. Worse,
   `IoConfigMode::Overwrite` *clears every PIOSEL bit not in the mask*, so
   manual pre-configuration is wiped by the constructor.

Also: `board/baosec.rs::setup_pmic_irq()` configures **(PB, 15)** as an IRQ
input — a leftover from the reference board. Do not call it; it fights the LED
driver for this exact pin.

## The badge resets every ~60s on battery: it is a watchdog

`bao1x-boot/boot1/src/platform/bao1x/bao1x.rs`, gated on `oem-baosec-lite`
(what this badge builds as):

```rust
if iox.get_gpio_pin_value(PA, 4) == IoxValue::Low {   // PA4 = VBUS detect
    let mut wdt = bao1x_hal::wdt::Wdt::new();
    wdt.enable((50_000_000 / 2) * 60, true);          // 60s, RESET enabled
}
```

Nothing in xous-core feeds it on bao1x — `xous-ticktimer`'s `watchdog` feature
covers precursor/hosted/renode only. So: **resets roughly every 60 seconds on
batteries, never on USB.** Either feed it (`Wdt::new().feed()` in your main
loop, which keeps the safety net) or `disable()` it.



Everything here needs **developer mode**, which is **irreversible**: `boot1`
erases the device's initial secrets and increments a one-way counter the first
time it validates a developer-signed image. The light-pattern exchange stops
working permanently. Confirmed on hardware — `audit` afterwards reports:

```
== IN DEVELOPER MODE ==
Collateral erased
Next stage: key 3/3 (dev ) -> 60060000
** System did not meet minimum requirements for security **
```

## Build

```bash
# WSL / Linux, one time
sudo apt install -y build-essential          # cargo build scripts need a host cc
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
git clone https://github.com/betrusted-io/xous-core.git && cd xous-core
git fetch --tags                             # image builds fail confusingly without tags
cargo xtask install-toolkit --force          # --force: skips silently without a TTY

cargo xtask baosec-lite                      # NOT `baosec` -- see below
```

Apply the patches in `patches/` first (see "What we had to change").

## Target: `baosec-lite`, not `baosec`

The DC34 badge is the **lite** variant. `bao1x-boot/boot1/Cargo.toml` has
`default = ["oem-baosec-lite"]`, and the lite board remaps the camera power-down
pin, moves the peripheral reset and inverts its polarity. A plain `baosec` image
fails hardware bring-up and `boot1` drops back to the bootloader — which looks
exactly like "it won't boot".

## Flashing

Hold `PROG` **while** plugging in USB-C → a `BAOCHIP` volume mounts → drag
`loader.uf2`, `xous.uf2`, `swap.uf2` onto it → press `PROG` to commit.

Two things that cost us a lot of time:

* **`bootwait`.** After flashing, `boot1` may sit in the bootloader forever
  rather than booting. That is the `bootwait` flag, not a broken image. On the
  bootloader console: `bootwait disable`, then `boot`.
* **The badge runs from two AA cells, so unplugging USB is NOT a reset.** A
  wedged app survives a replug. To actually reset, hold `PROG` while plugging in
  (or pull a cell). Many "it's still broken after I replugged" moments were this.

Do not open the bootloader's serial console casually — asserting DTR makes
`boot1` exit and boot the application. Flashing only needs the mass-storage
volume.

## What we had to change (patches/)

### `xtask-package-list.patch` — the stock target ships no application

`cargo xtask baosec` builds **system services only**. xtask's own comments list
`bao-console` and `[planned] vault application`, and neither is in the package
list. A freshly flashed badge therefore comes up with USB enumerated
(`usb-bao1x` IS included), a silent console (`bao-console` is NOT) and a blank
screen (`bao-video` runs, but nothing tells it what to draw). That is not a
fault; there is simply no app.

This patch adds `bao-console` to RRAM (where the upstream
`baosec-improper-keystore` target puts it) and moves `modals`/`pddb` to swap to
make room. Placing `bao-console` in **swap** instead does not work.

`keystore` does **not** need adding — `baosec_common` already adds it
explicitly, and it must be PID 3.

### `bio-oem-led-deadlock.patch` — the one that actually blocks LED work

`services/bao1x-hal-service/src/servers/bio.rs`, under `oem-baosec-lite`:

```rust
let sid = xns.register_name(BIO_SERVER_NAME, None).unwrap();   // name registers
...
let led_conn = xns.request_connection_blocking("_oem_led_").unwrap();  // BLOCKS
```

**Nothing in the open-source tree ever registers `_oem_led_`** — grep finds it
in exactly one place, this connect. It is DEF CON's proprietary LED manager,
shipped only inside `dc34-vault`. So the BIO service publishes its name and then
parks forever *before* entering its message loop.

The effect is nasty to diagnose: `Bio::new()` **succeeds** (the name is
registered) and the very next call blocks forever. Any `Ws2812` use hangs the
calling thread with no timeout. Swapping `send()` for `send_async()` changes
nothing, because the hang is upstream of any FIFO traffic.

The patch makes that connection optional. `led_conn` is used in exactly one
place — `BioOp::PrepFreqChange`, to pause LED rendering during a clock change so
you don't get brightness glitches — which is cosmetic.

### `bao-console-ws2812-feature.patch`

`bio-lib` gates the driver behind its own feature:

```rust
#[cfg(feature = "ws2812")]
pub mod ws2812;
```

`bao-console` depends on `bio-lib` without enabling it →
`error[E0432]: unresolved import bio_lib::ws2812`.

### `bao-console-register-cmds.patch`

Registers the `led` and `screen` commands, following the 4-step procedure
documented in `cmds.rs` itself.

## The panel boots at ZERO BRIGHTNESS

This one cost hours. `bao-video` initialises the display, `gfx.bitmap()` renders
correctly, and the server returns success — **and you see nothing**, because
brightness starts at 0. Call `gfx.brightness(200)` and the image is there,
already drawn.

If the screen looks dead, suspect brightness before suspecting your drawing
code. A second trap in the same area: `Rectangle::new()` defaults to
`DrawStyle { fill_color: None, stroke_color: None, stroke_width: 0 }`, so a
shape drawn that way renders nothing while still returning `Ok` — use
`Rectangle::new_with_style`.

## Status

**Working:** own firmware built, signed and booting; `bao-console` REPL live over
USB-serial with `led` and `screen` registered; `screen box` round-trips through
the graphics server ("box drawn").

**Screen — WORKING.** `screen invaders` draws the frame on the panel. The only
thing that was ever wrong was brightness (see above); the render path was
correct from the first attempt.

**Interactive game — draws, but the frame pacing is broken.** `invaders` is
registered and runs, but returns "game over - score 0" almost immediately: the
loop burns all 3000 frames in a couple of seconds, so `tt.sleep_ms(90)` is not
actually delaying. Fix the pacing (check the `Ticktimer` handle is valid inside
the command, or use an absolute deadline rather than per-frame sleeps) and the
game should be playable. Input is already wired: a helper thread runs
`get_keys_blocking()` and publishes to atomics, because that call blocks.
Keys are `'←'`, `'→'`, `'🔥'` (centre/fire), `'∴'` (select/quit).

**LEDs — still dark, but now for a known reason.** The corrected scan walks all
32 BIO-mappable pins (PB0-15 -> bio bit `15-pin`, PC0-15 -> bio bit `pin+16`,
every other port returns `None`) calling `set_bio_bit_from_port_and_pin()`,
which performs the **AF1 mux** that `Ws2812::new()` alone does not:

```rust
self.set_alternate_function(port, pin, IoxFunction::AF1);   // the missing half
self.csr.wo(iox::SFR_PIOSEL, ... | (1 << bio_bit));
```

The scan completes cleanly on every pin and **nothing lights**. Since PB and PC
are the *only* BIO-capable ports, that is fairly strong evidence the LED ring is
**not driven over BIO from a PB/PC pad at all** — consistent with there being no
LED code anywhere in `bao1x-hal`. Remaining hypotheses, in order:

1. The ring needs a power rail enabled first (compare `setup_oled_power_pin` /
   `setup_trng_power_pin`, or an AXP2101 PMIC LDO) — the pads may be driven fine
   while the LEDs have no supply.
2. `LedVariant::C` timing or `N_LEDS = 10` is wrong, so data is shifted out but
   never latched as valid colour.
3. The ring hangs off the *outer* badge PCB through the board-to-board
   connector on a pin the open tree never describes.

Extracting the answer from the stock `xous.uf2` means disassembly, not strings —
the pin is a constant in code. Strings do confirm the surrounding machinery:
`_oem_led_`, `dc34_console::bio::lightgenes`, the gene struct
`cd_period cd_rate cd_dir sat hue_ratedir hue_bound chaser nonlin hue_base`,
ops `ForceSetGene Syngamy SetTestRate Pause Autogamy`, and
`_Vault2_ bio.pins JackEyesBaselineDeadlock`.

There is also **no LED support anywhere in `bao1x-hal`** — the board file has
setup functions for display, camera, I2C, keyboard, USB, TRNG power and DCDC
rails, and zero LED references. The pin mapping exists only in the proprietary
`_oem_led_` server.

Strings recovered from the stock `xous.uf2` confirm the shape of it:
`_oem_led_`, `dc34_console::bio::lightgenes`, `src/bio/lightgenes/mod.rs`, the
gene struct `cd_period cd_rate cd_dir sat hue_ratedir hue_bound chaser nonlin
hue_base`, the ops `ForceSetGene Syngamy SetTestRate Pause Autogamy`, and
`_Vault2_ bio.pins JackEyesBaselineDeadlock` — "Eyes" matching the 2-eye + ring
layout.

## Restoring the stock firmware

Official images are downloadable, so the badge can always be put back (minus the
erased secrets):

```
https://ci.betrusted.io/releases/latest/baochip/dc34-badge/latest.zip
```

Flash `loader.uf2` / `xous.uf2` / `swap.uf2` the same way. Its `swap.uf2` is
~2288 KB versus our ~1694 KB; the difference is `dc34-vault` and the LED
manager.
