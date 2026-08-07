# -*- coding: utf-8 -*-
"""
sim.py - work out the panel's true bit/byte order WITHOUT more hardware round trips.

Encoding and decoding are both permutations of pixel positions. If I encode an
image with order O and the panel decodes with true order T, the glass shows

    D = decode_T( encode_O( I ) )

I already have photos of D for O in {lsb, msb, rev32}. So: render D for every
candidate T, and whichever matches the photo identifies T. Then encode with
O = T and the image is correct.

Candidate orders (bn = x + y*128, w = bn//32, m = bn%32):
    lsb    byte = bn//8,        bit = bn%8
    msb    byte = bn//8,        bit = 7-(bn%8)
    bswap  byte = 4w+3-(m//8),  bit = bn%8        (bytes of each u32 reversed)
    rev32  byte = 4w+(31-m)//8, bit = (31-m)%8    (== bswap AND msb together)
"""
import sys

sys.path.insert(0, ".")
from PIL import Image

W = H = 128
FB = W * H // 8
ORDERS = ("lsb", "msb", "bswap", "rev32")


def pos(bn, order):
    if order == "lsb":
        return bn >> 3, bn & 7
    if order == "msb":
        return bn >> 3, 7 - (bn & 7)
    w, m = bn >> 5, bn & 31
    if order == "bswap":
        return (w << 2) + 3 - (m >> 3), bn & 7
    k = 31 - m                                  # rev32
    return (w << 2) + (k >> 3), k & 7


def encode(img, order):
    px = img.load()
    fb = bytearray(FB)
    for y in range(H):
        for x in range(W):
            if px[x, y]:                        # ink
                b, i = pos(x + y * W, order)
                fb[b] |= 1 << i
    return fb


def decode(fb, order):
    img = Image.new("1", (W, H), 0)
    px = img.load()
    for y in range(H):
        for x in range(W):
            b, i = pos(x + y * W, order)
            if (fb[b] >> i) & 1:
                px[x, y] = 1
    return img


def main():
    import gen                                   # reuse the same artwork
    src = gen.DESIGNS["peach"]()
    sent = sys.argv[1] if len(sys.argv) > 1 else "rev32"

    fb = encode(src, sent)
    tiles = [("SENT as " + sent, src)]
    for t in ORDERS:
        tiles.append(("panel if true=" + t, decode(fb, t)))

    tw = 200
    sheet = Image.new("RGB", (tw * len(tiles), tw + 18), "white")
    from PIL import ImageDraw
    d = ImageDraw.Draw(sheet)
    for i, (label, im) in enumerate(tiles):
        # invert so it matches how the panel looks: ink dark on a lit field
        vis = im.convert("L").point(lambda v: 0 if v else 255).convert("RGB")
        sheet.paste(vis.resize((tw, tw), Image.NEAREST), (i * tw, 18))
        d.text((i * tw + 4, 4), label, fill="black")
    sheet.save("sim_sheet.png")
    print("wrote sim_sheet.png  (sent as %s)" % sent)


main()
