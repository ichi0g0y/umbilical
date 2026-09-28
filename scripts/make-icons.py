#!/usr/bin/env python3
"""Draw the Umbilical icons with no extra libraries.

- src-tauri/icons/source.png         : 1024x1024 app icon for macOS and Linux (feed it
  to `tauri icon`): an orange ring on a dark rounded square
- src-tauri/icons/source-windows.png : 1024x1024 app icon for Windows: a white ring only
- src-tauri/icons/tray.png           : 64x64 black ring, a template icon for the macOS
  menu bar (macOS makes it black or white)
- src-tauri/icons/tray-white.png     : 64x64 white ring for the Windows and Linux tray
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


def ring(x, y):
    """Distance to a ring in the middle, in unit space (-1..1)."""
    return abs(math.hypot(x, y) - 0.42) - 0.09


def draw(size, app_bg, rgb=(0, 0, 0), zoom=1.0):
    """app_bg: orange ring on the dark square. Else only the ring in `rgb`.
    zoom > 1 draws the ring bigger, so it fills more of the image."""
    px = [0] * (size * size * 4)
    aa = 2.0 / size / zoom
    for j in range(size):
        y = ((j + 0.5) / size * 2 - 1) / zoom
        for i in range(size):
            x = ((i + 0.5) / size * 2 - 1) / zoom
            g = cover(ring(x, y), aa)
            k = (j * size + i) * 4
            if app_bg:
                bg = cover(sd_round_box(x, y, 0.80, 0.22), aa)
                if bg == 0:
                    continue
                t = (y + 1) / 2
                base = (int(24 + 10 * t), int(30 + 12 * t), int(46 + 22 * t))
                fg = (236, 128, 84)
                mixed = [round(base[c] * (1 - g) + fg[c] * g) for c in range(3)]
                px[k:k + 4] = mixed + [round(255 * bg)]
            else:
                px[k:k + 4] = list(rgb) + [round(255 * g)]
    return px


def ring(x, y):
    """Distance to a ring in the middle, in unit space (-1..1)."""
    return abs(math.hypot(x, y) - 0.42) - 0.09


WHITE = (255, 255, 255)
OUT.mkdir(parents=True, exist_ok=True)
files = {
    "source.png": draw(1024, True),
    "source-windows.png": draw(1024, False, WHITE, zoom=1.8),
    "tray.png": draw(64, False, zoom=1.8),
    "tray-white.png": draw(64, False, WHITE, zoom=1.8),
}
for name, px in files.items():
    size = int((len(px) // 4) ** 0.5)
    png(OUT / name, size, size, px)
    print("wrote", OUT / name)
