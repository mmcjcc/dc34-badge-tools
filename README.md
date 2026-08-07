# DC34 Badge Tools

Host-side tooling and reverse-engineering notes for the **DEF CON 34 badge**
(Baochip-1x SoC running Xous). Everything here talks to the badge over its
USB-serial console — **no firmware flashing, no developer mode, no secrets touched.**

> **Why that matters:** loading a developer-signed image at *any* stage of the boot
> chain erases the device's initial secrets and increments a **one-way counter** that
> permanently flags the part as a developer device. It is irreversible. None of the
> tools in this repo go anywhere near that path.

---

## The device

Plugged in over USB-C the badge enumerates as a composite device, **VID `1d50` / PID `6198`**:

| Interface | Function |
|---|---|
| `MI_00` | HID — FIDO / security token (the "vault" side) |
| `MI_01` | HID — keyboard |
| `MI_02` | USB-serial (CDC-ACM) — **the console**, e.g. `COM4`, `/dev/ttyACM0` |

Serial settings: **1000000 baud (1 Mbaud)**, 8-N-1, no flow control, `\n` line ending.
Every line you send is echoed back prefixed with `[console] ` before the real reply.

Hardware (confirmed on-device): VexRiscv RV32-IMAC @350 MHz with an Sv32 MMU, 4x PicoRV32
"BIO" I/O coprocessors @700 MHz, **SH1107 128x128 monochrome OLED over SPI/UDMA**,
LIS2DH12 accelerometer @ I2C `0x19`, GC2145 camera @ I2C `0x3C`, 2x SAO ports, 2x AA cells.

---

## Console command surface

The badge advertises exactly five verbs. Sending anything unrecognised prints the list:

```
Commands: echo, ver, test, image, bio
```

A sweep of ~46 plausible hidden top-level verbs (`cam`, `led`, `vault`, `pddb`,
`keystore`, `poke`, `qr`, ...) found **no** undocumented top-level commands — the DC34
build strips the extra debug verbs present in upstream `bao-console`.

### Verified subcommands

`test` advertises only four, but its help says *"see code for other test commands."*
Sweeping the subcommand space against the help-line oracle turned up several that are
**not advertised**:

| Command | Advertised | Effect |
|---|---|---|
| `test proc` | yes | Xous process table (PID, state, satp, connections) |
| `test freemem` | yes | Per-process RAM usage |
| `test interrupts` | yes | IRQ -> process/handler table |
| `test bootwait [check\|enable\|disable]` | yes | Secure boot-wait flag |
| `test time` | **no** | Epoch/UTC/local time, sleeps 3 s, reprints |
| `test wfi` | **no** | Forces a suspend (`ForceWfi`) — console drops briefly |
| `test temp` | **no** | On-die ADC temperature |
| `test hw` | **no** | Factory self-test: VBAT / VBUS, emits `HW.PASS` |
| `ver xous` | yes | Xous version string |

The console emits machine-readable markers around key values, wrapped as
`_|TT|_KEY,VALUE,_|TE|_` — e.g. `_|TT|_HW.VBAT,990,_|TE|_`. That framing is how the
official host tools scrape results.

### Observed process table

`test proc` shows the full Xous service set: `kernel`, `xous-swapper`, `keystore`,
`xous-ticktimer`, `xous-log`, `xous-names`, `usb-bao1x`, `bao1x-hal-service`, `modals`,
`pddb`, `bao-video`, `dc34-console`, `dc34-vault`.

### Commands deliberately NOT run here

Reported to exist on some builds and destructive: `test reset` (deletes the `dc34` PDDB
dict), `test k0` / `test fakek0` (overwrite the master key), `test k0check` (logs the raw
secret), `test jig` (factory mode), `test shipmode` (PMIC power-off), `test wdt`
(watchdog reboot), `test deep` (deep sleep). Several of these are feature-gated out of the
production build and return the generic help line.

---

## The `image` protocol (reverse-engineered)

There is no public `dc34-image` tool. The wire format was recovered from a comment in
[`baochip/bio-loader`](https://github.com/baochip/bio-loader)'s `bio_loader.py`, which
states its chunk format is *"identical to send_image.py"* and names the payload field
**pixel data**:

```
70-byte chunk, then base64, sent as:  image <base64>\n

  [0:2]   u16  chunk index          (big-endian)
  [2:66]  u8 * 64  pixel data
  [66:70] u32  CRC-32 over [0:66]   (big-endian)
```

Device replies: `OK` (chunk stored), `ERR` (bad base64 / length / CRC), `SUCCESS`
(final chunk — image committed). `image clear` reverts to the default and replies `CLEAR`.

**Confirmed empirically:** the framebuffer is 128 x 128 x 1bpp = 2048 bytes = **exactly 32
chunks**, and the device returns `SUCCESS` on chunk index 31.

### Framebuffer layout

Derived from `put_pixel()` in `libs/bao1x-hal/src/sh1107.rs` (betrusted-io/xous-core):

```rust
let bitnum = (p.x + p.y * COLUMN) as usize;   // COLUMN = 128
self.buffer[bitnum / 32] |= 1 << (bitnum % 32);
```

The buffer is `[u32; 512]`. Because RISC-V is little-endian, the u32 indexing collapses to
a plain linear bitmap:

```
byte = (x + y*128) / 8        bit = (x + y*128) % 8       (LSB = leftmost pixel)
```

i.e. **row-major, 16 bytes per row, LSB-first**.

### Panel addressing

`draw()` uses `SetAddressMode(AddressMode::Column)` (vertical addressing) and ships the
buffer as **128 columns x 16 bytes**:

```rust
let chunk_size = 16;
for page in 0..128 { SetPageAddress(0); SetColumnAddress(page); /* 16 bytes */ }
```

Working the mapping through, `put_pixel(x, y)` lands on panel **column y, row x**.

**Confirmed on hardware:** despite that transposition in the addressing, no compensating
transform is needed — `transform = id` renders upright and correctly handed. Verified by
uploading a large asymmetric "F" with a `TOP` label and a top-left corner marker: `id`
renders it correctly, and `fliph` renders it mirrored. `tools/gen_image.py` still exposes
`transform` (`id`, `transpose`, `rot90/180/270`, `fliph`, `flipv`) for experimentation.

### Polarity

Init runs `SetDisplayMode(DisplayMode::WhiteOnBlack)` (`0xA6|1` = `0xA7`, SH1107 reverse
mode) and `clear()` fills `0xFFFFFFFF`.

**Confirmed on hardware:** bit `1` renders **dark** and bit `0` renders **lit**. So
`polarity = inv` (ink bit = 1) draws dark artwork on a lit field, which is what the
sample designs assume. Use `norm` for glowing artwork on a dark field — easier on the
battery, since the field is then unlit.

---

## Usage

```bash
# render a design, encode it, and emit 32 "image <base64>" lines
python tools/gen_image.py out.txt F transpose inv

# stream it to the badge
powershell -File tools/send_image.ps1 -Port COM4 -File out.txt
```

`tools/serial.ps1` is a minimal one-shot console helper:

```bash
powershell -File tools/serial.ps1 -Port COM4 -Send "test hw" -Baud 1000000
```

---

## The light-pattern challenge

The badge's headline interaction is a **light pattern exchange**: press the middle button
to reveal a nonce QR, a donor scans it, you scan their encrypted pattern QR, then accept or
roll back the new pattern.

Strings recovered from research around the LED subsystem describe it in explicitly
**genetic** terms — `Haploid`, `Syngamy` (the fusion of two gametes), `genome`, `mutate`,
`autogamy` (self-fertilisation), and a gene "type" drawn from the DEF CON attendee classes:
`human`, `goon`, `comm`, `village`, `ctf`, `other`, `uber`. Badges are printed with their
class (e.g. **HUMAN**), which matches that enum exactly.

So the exchange is not a simple pattern copy — it reads as **breeding**: two badges
contribute haploid gene sets and the offspring pattern is a recombination, with a
mutation rate. That is the actual puzzle.

*Status:* the LED control verbs (`hue`, `autogamy`, `transmute`, `mate`, `bt`, `rate`) are
**absent from this production build** — every one returns the generic `test` help line.
They appear to be compiled out behind `misc-test` / `qa-test` feature gates, consistent
with `test accel`, `test adc`, `test cam` and `test qrshow` also being absent.

---

## Licence / scope

Research on hardware I own. No secrets extracted, no attestation broken, nothing flashed.
