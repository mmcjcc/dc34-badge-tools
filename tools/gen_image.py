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

    # ---- hair: THICK ring so blonde reads light, face inside ------------
    # Everything here is >=4px: fine linework does not survive on this panel.
    d.ellipse([22, 24, 106, 108], fill=1)      # hair silhouette (solid)
    d.ellipse([28, 30, 100, 102], fill=0)      # hollow it out -> thick ring
    d.ellipse([20, 54, 50, 122], fill=1)       # long lobe L
    d.ellipse([26, 60, 44, 116], fill=0)
    d.ellipse([78, 54, 108, 122], fill=1)      # long lobe R
    d.ellipse([84, 60, 102, 116], fill=0)
    d.ellipse([40, 32, 88, 94], fill=0)        # face opening
    d.arc([40, 28, 88, 76], 185, 355, fill=1, width=6)   # fringe

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


def encode(img, ink_is_one, msb_first=True):
    """ink_is_one=True  -> PIL ink(1) sets the panel bit (set bit = dark)
       msb_first=True   -> bit 7 is the LEFTMOST pixel of each byte.

    Confirmed on hardware. Reading put_pixel() in sh1107.rs suggests LSB-first
    (bitnum = x + y*128; buffer[bitnum/32] |= 1 << (bitnum%32)), but the panel
    actually latches each byte MSB-first. Get this wrong and every group of 8
    horizontal pixels is mirrored: solid fills and thick bars still look almost
    right, while curves and fine detail shatter into a mirrored kaleidoscope.
    A calibration frame of solid 0xFF bytes CANNOT detect this -- 0xFF is
    symmetric under bit reversal. Use an asymmetric byte (see make_cal.py)."""
    px = img.load()
    fb = bytearray(FB)
    for y in range(H):
        for x in range(W):
            ink = bool(px[x, y])
            if ink == ink_is_one:
                bn = x + y * W
                bit = (7 - (bn & 7)) if msb_first else (bn & 7)
                fb[bn >> 3] |= 1 << bit
    return bytes(fb)


def main():
    out, design, xf = sys.argv[1], sys.argv[2], sys.argv[3]
    pol = sys.argv[4] if len(sys.argv) > 4 else "inv"   # inv => ink bit = 1
    bits = sys.argv[5] if len(sys.argv) > 5 else "msb"  # msb => bit7 leftmost
    img = DESIGNS[design]()
    # Preview rendered as the panel will actually show it: with pol=inv the
    # ink bit is 1 which reads DARK on a lit field, so invert for the preview.
    prev = img
    if pol == "inv":
        prev = ImageOps.invert(img.convert("L")).convert("1")
    prev.resize((384, 384), Image.NEAREST).save(out + ".preview.png")
    img = TRANSFORMS[xf](img)
    fb = encode(img, ink_is_one=(pol == "inv"), msb_first=(bits == "msb"))
    lines = []
    for i in range(FB // CHUNK):
        payload = struct.pack(">H", i) + fb[i * CHUNK:(i + 1) * CHUNK]
        wire = payload + struct.pack(">I", zlib.crc32(payload) & 0xFFFFFFFF)
        lines.append("image " + base64.b64encode(wire).decode())
    open(out, "w").write("\n".join(lines) + "\n")
    print("wrote %s  design=%s transform=%s chunks=%d" % (out, design, xf, len(lines)))


main()
