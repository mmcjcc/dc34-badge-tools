# -*- coding: utf-8 -*-
"""
sim2.py - identify the panel's true order by matching ALL observed photos.

Grid: one ROW per candidate true order T, one COLUMN per order I actually sent.
Cell (T, O) = what the glass should have shown. The row that matches all three
photos identifies T.

Observed, in order:
  sent lsb   -> heavy 8px mirroring; kaleidoscope; crown doubled
  sent msb   -> smooth curves, but crown still doubled
  sent rev32 -> crown SINGLE and centred, body jagged/sheared
"""
from PIL import Image, ImageDraw
from sim import ORDERS, encode, decode
import gen

src = gen.DESIGNS["peach"]()
SENT = ("lsb", "msb", "rev32")

tw, pad, lab = 190, 4, 20
sheet = Image.new("RGB", (tw * len(SENT) + 150, (tw + pad) * len(ORDERS) + lab), "white")
d = ImageDraw.Draw(sheet)
for c, o in enumerate(SENT):
    d.text((150 + c * tw + 40, 5), "sent " + o, fill="black")

for r, t in enumerate(ORDERS):
    y = lab + r * (tw + pad)
    d.text((6, y + tw // 2), "true=" + t, fill="black")
    for c, o in enumerate(SENT):
        img = decode(encode(src, o), t)
        vis = img.convert("L").point(lambda v: 0 if v else 255).convert("RGB")
        sheet.paste(vis.resize((tw, tw), Image.NEAREST), (150 + c * tw, y))

sheet.save("sim_grid.png")
print("wrote sim_grid.png")
