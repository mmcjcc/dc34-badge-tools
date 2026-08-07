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
import base64, struct, sys, zlib
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

    # ---- hair: outlined (blonde reads light), face inside ---------------
    d.ellipse([26, 27, 102, 103], outline=1, width=2)
    d.ellipse([24, 56, 50, 120], outline=1, width=2)   # long lobe L
    d.ellipse([78, 56, 104, 120], outline=1, width=2)  # long lobe R
    d.ellipse([43, 34, 85, 92], outline=1, width=2)    # face
    d.arc([43, 30, 85, 70], 185, 355, fill=1, width=3) # fringe
    for sx, sy, ex, ey in ((33, 70, 39, 108), (41, 66, 45, 100),
                           (95, 70, 89, 108), (87, 66, 83, 100)):
        d.line([(sx, sy), (ex, ey)], fill=1, width=1)  # hair strands

    # ---- crown: band + three points --------------------------------------
    d.rectangle([46, 23, 82, 32], fill=1)
    d.polygon([(46, 25), (52, 8), (58, 25)], fill=1)
    d.polygon([(58, 25), (64, 3), (70, 25)], fill=1)
    d.polygon([(70, 25), (76, 8), (82, 25)], fill=1)
    d.ellipse([61, 9, 67, 15], fill=0)
    d.ellipse([49, 15, 54, 20], fill=0)
    d.ellipse([74, 15, 79, 20], fill=0)
    d.rectangle([50, 26, 78, 29], fill=0)

    # ---- gown -------------------------------------------------------------
    d.rectangle([58, 90, 70, 99], fill=0, outline=1)   # neck
    d.polygon([(56, 97), (72, 97), (103, 127), (25, 127)], fill=0, outline=1)
    d.ellipse([21, 93, 49, 121], fill=0, outline=1)    # puff sleeve L
    d.ellipse([79, 93, 107, 121], fill=0, outline=1)   # puff sleeve R
    d.line([(29, 122), (99, 122)], fill=1, width=2)
    d.ellipse([59, 98, 69, 108], fill=1)               # brooch
    d.ellipse([62, 101, 66, 105], fill=0)

    # ---- face ------------------------------------------------------------
    d.ellipse([52, 56, 60, 68], fill=1)                # eyes
    d.ellipse([68, 56, 76, 68], fill=1)
    d.ellipse([54, 59, 57, 63], fill=0)                # catchlights
    d.ellipse([70, 59, 73, 63], fill=0)
    d.arc([57, 70, 71, 82], 20, 160, fill=1)           # smile

    d.rectangle([0, 0, W - 1, H - 1], outline=1)
    return img


DESIGNS = {"F": design_F, "skull": design_skull,
           "grid": design_grid, "peach": design_peach}

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


def encode(img, ink_is_one):
    """ink_is_one=True  -> PIL ink(1) sets the panel bit (bit 1 = lit)
       ink_is_one=False -> PIL ink(1) clears the bit (bit 0 = lit)"""
    px = img.load()
    fb = bytearray(FB)
    for y in range(H):
        for x in range(W):
            ink = bool(px[x, y])
            if ink == ink_is_one:
                bn = x + y * W
                fb[bn >> 3] |= 1 << (bn & 7)
    return bytes(fb)


def main():
    out, design, xf = sys.argv[1], sys.argv[2], sys.argv[3]
    pol = sys.argv[4] if len(sys.argv) > 4 else "inv"   # inv => ink bit = 1
    img = DESIGNS[design]()
    # Preview rendered as the panel will actually show it: with pol=inv the
    # ink bit is 1 which reads DARK on a lit field, so invert for the preview.
    prev = img
    if pol == "inv":
        prev = ImageOps.invert(img.convert("L")).convert("1")
    prev.resize((384, 384), Image.NEAREST).save(out + ".preview.png")
    img = TRANSFORMS[xf](img)
    fb = encode(img, ink_is_one=(pol == "inv"))
    lines = []
    for i in range(FB // CHUNK):
        payload = struct.pack(">H", i) + fb[i * CHUNK:(i + 1) * CHUNK]
        wire = payload + struct.pack(">I", zlib.crc32(payload) & 0xFFFFFFFF)
        lines.append("image " + base64.b64encode(wire).decode())
    open(out, "w").write("\n".join(lines) + "\n")
    print("wrote %s  design=%s transform=%s chunks=%d" % (out, design, xf, len(lines)))


main()
