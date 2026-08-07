# -*- coding: utf-8 -*-
"""
make_cal.py <outfile>

Calibration frame: writes RAW framebuffer bytes, bypassing any pixel model,
so the photo tells us the true byte -> pixel mapping.

Bit 1 renders DARK on the lit field (confirmed), so a set byte = a dark mark.

  bytes   0..15  = 0xFF   -> "line A"
  bytes  64..79  = 0xFF   -> "line B"
  byte     1024  = 0xFF   -> midpoint dash

Interpretation of the resulting photo:
  * row-major, 16 bytes/row (what gen_image.py assumes)
        bytes 0-15   = pixels 0..127      -> full HORIZONTAL line across the TOP
        bytes 64-79  = pixels 512..639    -> horizontal line 4 rows below it
  * SH1107-native column/vertical addressing
        bytes 0-15   = column 0, pages 0-15 -> full VERTICAL line down the LEFT
        bytes 64-79  = column 4            -> vertical line 4 columns right
"""
import base64
import struct
import sys
import zlib

FB = 2048
CHUNK = 64

fb = bytearray(FB)
for i in range(0, 16):
    fb[i] = 0xFF
for i in range(64, 80):
    fb[i] = 0xFF
fb[1024] = 0xFF

# --- bit-order probe -------------------------------------------------------
# CRITICAL: 0xFF is symmetric under bit reversal, so the marks above cannot
# tell LSB-first from MSB-first. Rows 40 and 48 use an ASYMMETRIC byte so the
# ordering is directly readable: each 0xC0 is a 2px dash inside an 8px cell.
#   MSB-first (correct here) -> dash sits at the LEFT of each 8px cell
#   LSB-first                -> dash sits at the RIGHT of each 8px cell
for i in range(16):
    fb[40 * 16 + i] = 0xC0
# A single isolated 0xC0 at the far left of row 48 shows the absolute edge.
fb[48 * 16] = 0xC0

lines = []
for i in range(FB // CHUNK):
    payload = struct.pack(">H", i) + bytes(fb[i * CHUNK:(i + 1) * CHUNK])
    wire = payload + struct.pack(">I", zlib.crc32(payload) & 0xFFFFFFFF)
    lines.append("image " + base64.b64encode(wire).decode())

open(sys.argv[1], "w").write("\n".join(lines) + "\n")
print("wrote %s (%d chunks) - calibration frame" % (sys.argv[1], len(lines)))
