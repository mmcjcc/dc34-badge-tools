# Custom firmware for the DC34 badge

> **This code requires developer mode, and developer mode is irreversible.**
> Loading a developer-signed image at any stage of the boot chain erases the
> device's initial secrets and increments a one-way counter that permanently
> flags the part. The light-pattern exchange stops working. `apps-baosec/vault2`
> — the password manager / FIDO app the badge ships with — is replaced.
>
> Everything in `../tools/` works on a **stock, un-fused badge**. Everything in
> this directory does not. Know which side of that line you are on.

## Why these two projects

Both do things the stock firmware flatly refuses:

**`led_show.rs`** — drives the on-board RGB LEDs. On shipped firmware there is
*no* path to them: every LED verb (`hue`, `autogamy`, `transmute`, `mate`,
`rate`) is compiled out of the console, proven by contrast with a real
under-specified command:

```
> test bootwait
bootwait [check | enable | disable]      <- real command, own usage string
> test hue
test [proc] [freemem] ...                <- generic help = not compiled in
```

`bio tx` cannot reach the light engine either — FIFO3 backs up and times out,
so nothing drains it.

**`invaders.rs`** — a real interactive game. The stock console can only push
whole static framebuffers over serial (~1-3 s each, and every one is a PDDB
write), so gameplay is impossible without owning the app.

## Space Invaders: why this game and not an emulator

* It is **natively 1-bit monochrome**, so a 128x128 mono OLED is the correct
  medium. A colour console emulator would need dithering and look bad.
* It **tolerates a variable frame rate**. The badge's display SPI link times out
  and resets itself on stock firmware (`timeout in draw` -> `resetting display
  spim block`), stalling redraws for up to seconds. A twitch game would feel
  broken; an invader marching one step late does not.
* It needs **exactly three inputs**, and the badge exposes
  `KeyPress::{Left, Right, Center}`.

The sprite art and layout were validated on the real panel *before* any Rust was
written, by rendering frames on the host and pushing them over the serial
`image` command (`tools/gen_image.py invaders`). Rendering was solved on a stock
badge; only input and the game loop need firmware.

## The LED driver is public

No reverse engineering needed — `libs/bio-lib/src/ws2812.rs` in
betrusted-io/xous-core:

```rust
let mut leds = Ws2812::new(LedVariant::C, pin, None)?;
leds.send(&[rgb_to_u32(255, 0, 0), ...]);   // GRB packed, chain order
```

All sub-microsecond WS2812 bit timing runs on a BIO coprocessor core at a 150 ns
quantum, so the CPU only pushes 24-bit words into a FIFO. There is also a
prebuilt `colorwheel` BIO program in `libs/bio-lib/src/c/colorwheel/`.

**Open question — the LED pin.** The reference app
(`apps-dabao/dabao-console/src/cmds/ws2812.rs`) uses `LED_BIO_PIN = 5`, but that
is the *dabao* dev board. The badge's pin is not in the public tree, and the
DC34 technical report lists LED count and placement as "not public". So
`scan_for_led_pin()` walks the BIO pins flashing green until the ring lights —
the same empirical approach that cracked the framebuffer encoding. Hard-code the
answer afterwards.

## Polarity, again

`clear()` fills `0xFFFFFFFF` and a **set bit renders dark**, so a cleared screen
is black and sprites must be drawn with `ColorNative(0)` to glow. This is
inverted from intuition and is the same fact that made the host-side image work
confusing — see the root README's byte-order section.

## Build

```bash
# in WSL, one-time
sudo apt install -y build-essential          # cargo build scripts need a host cc
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
git clone https://github.com/betrusted-io/xous-core.git && cd xous-core
git fetch --tags                             # image builds fail confusingly without tags
cargo xtask install-toolkit --force          # --force: it skips silently without a TTY

cargo xtask baosec                           # produces loader.uf2 / xous.uf2 / swap.uf2
```

Flash by holding `PROG` while plugging in, then dragging the UF2 onto the
`BAOCHIP` volume. `boot0` is ROM, so a bad *application* image is always
recoverable — but the developer-mode fuse is not.

## Before you flash: archive the factory images

Hold `PROG`, plug in, and copy the shipped `loader.uf2` / `xous.uf2` /
`swap.uf2` off the `BAOCHIP` volume, plus the output of `audit` at the `boot1`
prompt. That baseline stops existing the moment the fuse trips, and it is the
only reference for what your badge shipped with.
