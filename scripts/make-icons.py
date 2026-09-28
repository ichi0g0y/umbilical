#!/usr/bin/env python3
"""Draw the Umbilical icons with no extra libraries.

- src-tauri/icons/source.png : 1024x1024 app icon (feed it to `tauri icon`)
- src-tauri/icons/tray.png   : 64x64 black template icon for the tray. It has the
  same shape as the app icon: a filled rounded square with the glyph cut out.
"""
import math
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "src-tauri" / "icons"


def png(path, w, h, pixels):
    raw = b"".join(b"\x00" + bytes(pixels[y * w * 4:(y + 1) * w * 4]) for y in range(h))

    def chunk(kind, data):
        c = struct.pack(">I", len(data)) + kind + data
        return c + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")
    path.write_bytes(data)


def cover(d, aa):
    """Signed distance -> coverage 0..1."""
    return max(0.0, min(1.0, 0.5 - d / aa))


def sd_round_box(x, y, half, r):
    qx, qy = abs(x) - half + r, abs(y) - half + r
    return math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - r


def sd_segment(x, y, ax, ay, bx, by):
    px, py, vx, vy = x - ax, y - ay, bx - ax, by - ay
    t = max(0.0, min(1.0, (px * vx + py * vy) / (vx * vx + vy * vy)))
    return math.hypot(px - vx * t, py - vy * t)


def glyph(x, y):
    """Distance to the glyph in unit space (-1..1): a ring, a cable and a plug."""
    ring = abs(math.hypot(x, y + 0.08) - 0.42) - 0.075
    a = math.radians(225)
    sx, sy = 0.42 * math.cos(a), 0.42 * math.sin(a) - 0.08
    cable = sd_segment(x, y, sx, sy, -0.72, 0.72) - 0.06
    plug = math.hypot(x - 0.30, y + 0.38) - 0.13
    return min(ring, cable, plug)


def draw(size, with_bg, zoom=1.0):
    """zoom > 1 draws the icon bigger, so the square fills more of the image."""
    px = [0] * (size * size * 4)
    aa = 2.0 / size / zoom
    for j in range(size):
        y = ((j + 0.5) / size * 2 - 1) / zoom
        for i in range(size):
            x = ((i + 0.5) / size * 2 - 1) / zoom
            g = cover(glyph(x, y), aa)
            k = (j * size + i) * 4
            if with_bg:
                bg = cover(sd_round_box(x, y, 0.80, 0.22), aa)
                if bg == 0:
                    continue
                t = (y + 1) / 2
                base = (int(24 + 10 * t), int(30 + 12 * t), int(46 + 22 * t))
                fg = (236, 128, 84)
                rgb = [round(base[c] * (1 - g) + fg[c] * g) for c in range(3)]
                px[k:k + 4] = rgb + [round(255 * bg)]
            else:
                bg = cover(sd_round_box(x, y, 0.80, 0.22), aa)
                px[k:k + 4] = [0, 0, 0, round(255 * bg * (1 - g))]
    return px


OUT.mkdir(parents=True, exist_ok=True)
png(OUT / "source.png", 1024, 1024, draw(1024, True))
png(OUT / "tray.png", 64, 64, draw(64, False, zoom=1.2))
print("wrote", OUT / "source.png", OUT / "tray.png")
