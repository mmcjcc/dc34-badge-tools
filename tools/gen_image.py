# -*- coding: utf-8 -*-
"""
gen.py <outfile> <design> <transform>

design    : F | skull | grid
transform : id | transpose | rot90 | rot180 | rot270 | fliph | flipv | atrans

Framebuffer encoding (from libs/bao1x-hal/src/sh1107.rs put_pixel):
    bitnum = x + y*128 ;  byte = bitnum//8 ; bit = bitnum%8  (LSB = leftmost)
Panel is driven in vertical addressing (128 columns x 16 bytes), so the
logical buffer appears TRANSPOSED on glass; `transform` compensates.

Polarity: init uses SetDisplayMode(WhiteOnBlack) = 0xA7 (reverse), and
clear() fills 0xFFFFFFFF -> bit 1 = dark. So ink must be 0, field 1.
"""
import base64, math, struct, sys, zlib
from PIL import Image, ImageDraw, ImageFont, ImageOps

W = H = 128
FB = W * H // 8
CHUNK = 64


def font(sz):
    for n in ("arialbd.ttf", "seguisb.ttf", "consolab.ttf", "arial.ttf"):
        try:
            return ImageFont.truetype("C:/Windows/Fonts/" + n, sz)
        except Exception:
            pass
    return ImageFont.load_default()


def design_F():
    """Giant asymmetric F + TOP label: disambiguates all 8 rotations/mirrors."""
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W - 1, H - 1], outline=1)
    d.rectangle([2, 2, 11, 11], fill=1)      # top-left corner marker
    f = font(14)
    t = "TOP"
    try:
        l, _, r, _ = d.textbbox((0, 0), t, font=f)
        w, off = r - l, l
    except Exception:
        w, off = d.textsize(t, font=f)[0], 0
    d.text(((W - w) // 2 - off, 3), t, font=f, fill=1)
    d.rectangle([30, 28, 50, 118], fill=1)   # stem
    d.rectangle([30, 28, 104, 46], fill=1)   # top arm
    d.rectangle([30, 64, 92, 80], fill=1)    # middle arm
    return img


def design_skull():
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W - 1, H - 1], outline=1)
    d.rectangle([2, 2, W - 3, H - 3], outline=1)
    d.rectangle([3, 3, W - 4, 21], fill=1)
    t = "DEF CON 34"
    f = font(15)
    try:
        l, _, r, _ = d.textbbox((0, 0), t, font=f)
        w, off = r - l, l
    except Exception:
        w, off = d.textsize(t, font=f)[0], 0
    d.text(((W - w) // 2 - off, 4), t, font=f, fill=0)

    cx, cy = 64, 60
    d.ellipse([cx - 26, cy - 23, cx + 26, cy + 15], fill=1)
    d.rectangle([cx - 13, cy + 9, cx + 13, cy + 25], fill=1)
    d.ellipse([cx - 18, cy - 13, cx - 5, cy + 2], fill=0)
    d.ellipse([cx + 5, cy - 13, cx + 18, cy + 2], fill=0)
    d.polygon([(cx, cy + 1), (cx - 4, cy + 9), (cx + 4, cy + 9)], fill=0)
    for gx in (-9, -3, 3, 9):
        d.line([(cx + gx, cy + 13), (cx + gx, cy + 24)], fill=0)
    d.line([(cx - 13, cy + 18), (cx + 13, cy + 18)], fill=0)
    d.line([(cx - 37, cy + 30), (cx + 37, cy + 40)], fill=1, width=4)
    d.line([(cx - 37, cy + 40), (cx + 37, cy + 30)], fill=1, width=4)

    t2 = "bao1x pwn"
    f2 = font(13)
    try:
        l, _, r, _ = d.textbbox((0, 0), t2, font=f2)
        w2, off2 = r - l, l
    except Exception:
        w2, off2 = d.textsize(t2, font=f2)[0], 0
    d.text(((W - w2) // 2 - off2, H - 22), t2, font=f2, fill=1)
    return img


def design_grid():
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    for i in range(0, 128, 16):
        d.line([(i, 0), (i, 127)], fill=1)
        d.line([(0, i), (127, i)], fill=1)
    d.rectangle([0, 0, 15, 15], fill=1)
    return img


def design_peach():
    """1-bit portrait: crown, long hair, gown. Ink=1."""
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    cx = 64

    # ---- hair: thick outline so blonde reads LIGHT, with a centre parting
    # Everything here is >=4px: fine linework does not survive on this panel.
    d.ellipse([22, 24, 106, 108], fill=1)      # hair silhouette (solid)
    d.ellipse([28, 30, 100, 102], fill=0)      # hollow it out -> thick ring
    d.ellipse([20, 52, 50, 124], fill=1)       # long lobe L
    d.ellipse([26, 58, 44, 118], fill=0)
    d.ellipse([78, 52, 108, 124], fill=1)      # long lobe R
    d.ellipse([84, 58, 102, 118], fill=0)
    d.ellipse([40, 32, 88, 94], fill=0)        # face opening
    d.arc([40, 26, 88, 74], 195, 345, fill=1, width=5)   # fringe
    d.line([(64, 27), (64, 40)], fill=1, width=4)        # centre parting
    d.arc([44, 26, 66, 58], 200, 300, fill=1, width=4)   # sweep L
    d.arc([62, 26, 84, 58], 240, 340, fill=1, width=4)   # sweep R

    # ---- crown: band + three points --------------------------------------
    d.rectangle([46, 23, 82, 32], fill=1)
    d.polygon([(46, 25), (52, 8), (58, 25)], fill=1)
    d.polygon([(58, 25), (64, 3), (70, 25)], fill=1)
    d.polygon([(70, 25), (76, 8), (82, 25)], fill=1)
    d.ellipse([61, 9, 67, 15], fill=0)
    d.ellipse([49, 15, 54, 20], fill=0)
    d.ellipse([74, 15, 79, 20], fill=0)
    d.rectangle([50, 26, 78, 29], fill=0)

    # ---- gown (bold) ------------------------------------------------------
    d.polygon([(54, 100), (74, 100), (104, 127), (24, 127)], fill=1)
    d.polygon([(58, 106), (70, 106), (94, 127), (34, 127)], fill=0)
    d.ellipse([18, 96, 48, 126], fill=1)               # puff sleeve L
    d.ellipse([24, 102, 42, 120], fill=0)
    d.ellipse([80, 96, 110, 126], fill=1)              # puff sleeve R
    d.ellipse([86, 102, 104, 120], fill=0)
    d.ellipse([56, 96, 72, 112], fill=1)               # brooch
    d.ellipse([61, 101, 67, 107], fill=0)

    # ---- face (bold) ------------------------------------------------------
    d.ellipse([50, 54, 61, 70], fill=1)                # eyes
    d.ellipse([67, 54, 78, 70], fill=1)
    d.ellipse([53, 58, 57, 64], fill=0)                # catchlights
    d.ellipse([70, 58, 74, 64], fill=0)
    d.arc([55, 70, 73, 86], 25, 155, fill=1, width=4)  # smile

    d.rectangle([0, 0, W - 1, H - 1], outline=1)
    return img


def design_mushroom():
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W - 1, H - 1], outline=1)
    d.pieslice([12, 16, 116, 112], 180, 360, fill=1)   # cap
    d.rectangle([12, 62, 116, 72], fill=1)
    for box in ([28, 32, 54, 58], [74, 32, 100, 58],
                [54, 20, 74, 40], [16, 48, 34, 66], [94, 48, 112, 66]):
        d.ellipse(box, fill=0)                          # spots
    d.rectangle([40, 72, 88, 118], fill=1)              # stem
    d.rectangle([47, 79, 81, 112], fill=0)
    d.ellipse([52, 86, 61, 100], fill=1)                # eyes
    d.ellipse([67, 86, 76, 100], fill=1)
    return img


def design_star():
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W - 1, H - 1], outline=1)
    pts = []
    for i in range(10):
        ang = math.radians(-90 + i * 36.0)
        rad = 58 if i % 2 == 0 else 25
        pts.append((64 + rad * math.cos(ang), 64 + rad * math.sin(ang)))
    d.polygon(pts, fill=1)
    d.ellipse([49, 54, 60, 72], fill=0)                 # eyes
    d.ellipse([68, 54, 79, 72], fill=0)
    d.arc([54, 74, 74, 90], 20, 160, fill=0, width=4)   # smile
    return img


def design_defcon():
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W - 1, H - 1], outline=1)
    d.rectangle([4, 4, W - 5, W - 5], outline=1)

    def ctr(y, text, sz):
        f = font(sz)
        try:
            l, _, r, _ = d.textbbox((0, 0), text, font=f)
            w, off = r - l, l
        except Exception:
            w, off = d.textsize(text, font=f)[0], 0
        d.text(((W - w) // 2 - off, y), text, font=f, fill=1)

    ctr(14, "DEF CON", 24)
    ctr(44, "34", 34)
    d.line([(20, 88), (108, 88)], fill=1, width=3)
    ctr(96, "HUMAN", 20)
    return img


# ---------------------------------------------------------------------------
# Space Invaders: authentic 1978 sprite art. A 1-bit 128x128 panel is the
# native medium for this game, so no dithering or greyscale fakery is needed.
# ---------------------------------------------------------------------------
SPRITES = {
    "squid": [
        "...##...",
        "..####..",
        ".######.",
        "##.##.##",
        "########",
        "..#..#..",
        ".#.##.#.",
        "#.#..#.#",
    ],
    "crab": [
        "..#.....#..",
        "...#...#...",
        "..#######..",
        ".##.###.##.",
        "###########",
        "#.#######.#",
        "#.#.....#.#",
        "...##.##...",
    ],
    # 11 wide to match the crab: at scale 2 that is 22px on a 23px pitch,
    # so adjacent columns keep a 1px gap instead of fusing into a blob.
    "octopus": [
        "...#####...",
        ".#########.",
        "###########",
        "##..###..##",
        "###########",
        "..##.#.##..",
        ".##..#..##.",
        "##.......##",
    ],
    "cannon": [
        "......#......",
        ".....###.....",
        ".....###.....",
        ".###########.",
        "#############",
        "#############",
        "#############",
        "#############",
    ],
    "ufo": [
        "....########....",
        "..############..",
        ".##############.",
        "##.###.##.###.##",
        "################",
        "..###..##..###..",
        "....##....##....",
    ],
}


def blit(img, sprite, x, y, scale=1, fill=1):
    """Draw string-art sprite at (x,y). '#' = ink."""
    d = ImageDraw.Draw(img)
    for row, line in enumerate(sprite):
        for col, ch in enumerate(line):
            if ch != "#":
                continue
            px, py = x + col * scale, y + row * scale
            if scale == 1:
                if 0 <= px < W and 0 <= py < H:
                    d.point((px, py), fill=fill)
            else:
                d.rectangle([px, py, px + scale - 1, py + scale - 1], fill=fill)


def design_invaders():
    """Attract-mode frame: the layout we'll port to firmware."""
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)

    _text_at(d, 34, 2, "SCORE", 11)
    _text_at(d, 34, 13, "0340", 11)
    _text_at(d, 96, 2, "HI", 11)
    _text_at(d, 96, 13, "9990", 11)

    blit(img, SPRITES["ufo"], 48, 26, scale=2)          # mystery ship

    # 3 rows x 5 columns at scale 2: sprite is 16px tall, so the row pitch must
    # exceed 16 or the rows fuse. 23px column pitch leaves a 1px gap.
    rows = ["squid", "crab", "octopus"]
    for r, kind in enumerate(rows):
        sp = SPRITES[kind]
        wpx = len(sp[0]) * 2
        for c in range(5):
            x = 6 + c * 23 + (22 - wpx) // 2
            blit(img, sp, x, 44 + r * 18, scale=2)

    # bunkers
    for b in range(4):
        bx = 12 + b * 28
        d.rectangle([bx, 98, bx + 18, 106], fill=1)
        d.rectangle([bx + 6, 103, bx + 12, 107], fill=0)

    blit(img, SPRITES["cannon"], 56, 110, scale=2)      # player
    d.line([(0, 127), (127, 127)], fill=1, width=1)     # ground
    for bx, by in ((30, 88), (92, 82)):                 # in-flight shots
        d.line([(bx, by), (bx, by + 5)], fill=1, width=2)
    return img


def _text_at(d, cx, y, text, sz, fill=1):
    f = font(sz)
    try:
        l, _, r, _ = d.textbbox((0, 0), text, font=f)
        w, off = r - l, l
    except Exception:
        w, off = d.textsize(text, font=f)[0], 0
    d.text((cx - w // 2 - off, y), text, font=f, fill=fill)


def _plumber(letter, tall=False):
    """Shared Mario/Luigi portrait. tall=True gives Luigi's longer face."""
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W - 1, H - 1], outline=1)

    top = 4 if tall else 10
    d.pieslice([24, top, 104, 74], 180, 360, fill=1)      # cap dome
    d.rectangle([24, 44, 104, 58], fill=1)
    d.ellipse([12, 46, 116, 72], fill=1)                  # brim
    d.ellipse([50, top + 12, 78, top + 40], fill=0)       # badge
    _text_at(d, 64, top + 14, letter, 24, fill=1)

    # sideburns
    d.polygon([(26, 68), (40, 68), (40, 100), (28, 90)], fill=1)
    d.polygon([(102, 68), (88, 68), (88, 100), (100, 90)], fill=1)

    ey = 78 if tall else 76
    d.ellipse([48, ey, 57, ey + 14], fill=1)              # eyes
    d.ellipse([71, ey, 80, ey + 14], fill=1)

    ny = 92 if tall else 88
    d.ellipse([54, ny, 74, ny + 16], fill=0, outline=1, width=3)   # nose

    my = 110 if tall else 106
    d.ellipse([26, my, 66, my + 14], fill=1)              # moustache
    d.ellipse([62, my, 102, my + 14], fill=1)
    d.rectangle([60, my + 2, 68, my + 12], fill=1)
    return img


def design_mario():
    return _plumber("M", tall=False)


def design_luigi():
    return _plumber("L", tall=True)


def design_toad():
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W - 1, H - 1], outline=1)
    # Cap: outlined (Toad's cap is white with red blotches) so it does not read
    # as one solid slab, and it sits ON the head rather than floating above it.
    d.pieslice([6, 10, 122, 108], 180, 360, fill=0, outline=1, width=4)
    d.line([(6, 59), (122, 59)], fill=1, width=4)         # cap brim
    for box in ([22, 20, 52, 50], [76, 20, 106, 50], [56, 14, 72, 30]):
        d.ellipse(box, fill=1)                            # blotches
    d.ellipse([14, 36, 32, 54], fill=1)
    d.ellipse([96, 36, 114, 54], fill=1)
    d.ellipse([38, 60, 90, 120], fill=0, outline=1, width=4)   # face
    d.ellipse([49, 76, 59, 94], fill=1)                   # eyes
    d.ellipse([69, 76, 79, 94], fill=1)
    d.arc([56, 96, 72, 110], 20, 160, fill=1, width=4)    # mouth
    d.ellipse([40, 92, 50, 102], fill=1)                  # cheeks
    d.ellipse([78, 92, 88, 102], fill=1)
    return img


def design_boo():
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W - 1, H - 1], outline=1)
    # Boo is white, so draw him as outline. Use arc(), not pieslice() --
    # pieslice also strokes the flat chord and leaves a bar across the face.
    d.arc([18, 14, 110, 106], 180, 360, fill=1, width=4)  # dome
    d.line([(20, 60), (20, 94)], fill=1, width=4)         # sides
    d.line([(108, 60), (108, 94)], fill=1, width=4)
    for i in range(4):                                    # scalloped tail
        x0 = 20 + i * 22
        d.arc([x0, 78, x0 + 22, 114], 0, 180, fill=1, width=4)
    d.ellipse([8, 62, 36, 90], fill=0, outline=1, width=4)    # arms
    d.ellipse([92, 62, 120, 90], fill=0, outline=1, width=4)
    d.ellipse([46, 48, 58, 68], fill=1)                   # eyes
    d.ellipse([70, 48, 82, 68], fill=1)
    d.chord([52, 72, 76, 96], 0, 180, fill=1)             # open mouth
    return img


def design_bright():
    """No ink at all -> whole panel lit. The 'on' frame for blinking."""
    return Image.new("1", (W, H), 0)


def design_dark():
    """All ink -> whole panel dark. The 'off' frame for blinking."""
    return Image.new("1", (W, H), 1)


def design_sos():
    img = Image.new("1", (W, H), 0)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, W - 1, H - 1], outline=1)

    def ctr(y, text, sz):
        f = font(sz)
        try:
            l, _, r, _ = d.textbbox((0, 0), text, font=f)
            w, off = r - l, l
        except Exception:
            w, off = d.textsize(text, font=f)[0], 0
        d.text(((W - w) // 2 - off, y), text, font=f, fill=1)

    ctr(12, "SOS", 40)
    # ... --- ...  drawn as bold morse
    y = 68
    x = 12
    for group in ((6, 6, 6), (18, 18, 18), (6, 6, 6)):
        for wdt in group:
            d.rectangle([x, y, x + wdt, y + 10], fill=1)
            x += wdt + 6
        x += 8
    d.line([(12, 96), (116, 96)], fill=1, width=2)
    ctr(102, "-- -.. .-", 16)
    return img


DESIGNS = {"F": design_F, "skull": design_skull,
           "bright": design_bright, "dark": design_dark, "sos": design_sos,
           "invaders": design_invaders,
           "mario": design_mario, "luigi": design_luigi,
           "toad": design_toad, "boo": design_boo,
           "grid": design_grid, "peach": design_peach,
           "mushroom": design_mushroom, "star": design_star,
           "defcon": design_defcon}

TRANSFORMS = {
    "id":        lambda im: im,
    "transpose": lambda im: im.transpose(Image.TRANSPOSE),
    "atrans":    lambda im: im.transpose(Image.TRANSVERSE),
    "rot90":     lambda im: im.transpose(Image.ROTATE_90),
    "rot180":    lambda im: im.transpose(Image.ROTATE_180),
    "rot270":    lambda im: im.transpose(Image.ROTATE_270),
    "fliph":     lambda im: im.transpose(Image.FLIP_LEFT_RIGHT),
    "flipv":     lambda im: im.transpose(Image.FLIP_TOP_BOTTOM),
}


def encode(img, ink_is_one, order="rev32"):
    """ink_is_one=True -> PIL ink(1) sets the panel bit (set bit = dark).

    `order` selects how a pixel index maps to (byte, bit). The framebuffer is
    [u32; 512], NOT a byte array, which is the whole difficulty:

      lsb    byte = bn/8, bit = bn%8         -- what put_pixel() literally implies
      msb    byte = bn/8, bit = 7-(bn%8)     -- fixes mirroring inside each byte
      rev32  the pixel occupies bit 31-(bn%32) of little-endian word bn/32
             -> byte = 4*(bn/32) + (31-(bn%32))/8, bit = (31-(bn%32))%8

    Confirmed on hardware: `rev32`. The two wrong answers fail distinctly, which
    is what makes this debuggable:
      * `lsb`  -> every 8 horizontal pixels mirrored in place. Thick bars survive
                  (a 20px bar spans ~3 cells and stays solid), curves shatter.
      * `msb`  -> bits right, but the 4 bytes of each word are in reverse order,
                  so shapes are smooth yet rearranged in 8px blocks within each
                  32px cell. Centred features appear DUPLICATED.
    Note `rev32` is exactly `msb` with the 4 bytes of each word reversed."""
    px = img.load()
    fb = bytearray(FB)
    for y in range(H):
        for x in range(W):
            ink = bool(px[x, y])
            if ink != ink_is_one:
                continue
            bn = x + y * W
            if order == "bswap":
                byte = ((bn >> 5) << 2) + 3 - ((bn & 31) >> 3)
                bit = bn & 7
            elif order == "rev32":
                k = 31 - (bn & 31)
                byte = ((bn >> 5) << 2) + (k >> 3)
                bit = k & 7
            elif order == "msb":
                byte, bit = bn >> 3, 7 - (bn & 7)
            else:
                byte, bit = bn >> 3, bn & 7
            fb[byte] |= 1 << bit
    return bytes(fb)


def main():
    out, design, xf = sys.argv[1], sys.argv[2], sys.argv[3]
    pol = sys.argv[4] if len(sys.argv) > 4 else "inv"   # inv => ink bit = 1
    bits = sys.argv[5] if len(sys.argv) > 5 else "bswap"  # lsb|msb|bswap|rev32
    img = DESIGNS[design]()
    # Preview rendered as the panel will actually show it: with pol=inv the
    # ink bit is 1 which reads DARK on a lit field, so invert for the preview.
    prev = img
    if pol == "inv":
        prev = ImageOps.invert(img.convert("L")).convert("1")
    prev.resize((384, 384), Image.NEAREST).save(out + ".preview.png")
    img = TRANSFORMS[xf](img)
    fb = encode(img, ink_is_one=(pol == "inv"), order=bits)
    lines = []
    for i in range(FB // CHUNK):
        payload = struct.pack(">H", i) + fb[i * CHUNK:(i + 1) * CHUNK]
        wire = payload + struct.pack(">I", zlib.crc32(payload) & 0xFFFFFFFF)
        lines.append("image " + base64.b64encode(wire).decode())
    open(out, "w").write("\n".join(lines) + "\n")
    print("wrote %s  design=%s transform=%s chunks=%d" % (out, design, xf, len(lines)))


if __name__ == "__main__":
    main()
