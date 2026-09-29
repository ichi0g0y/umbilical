#!/usr/bin/env python3
"""Draw the Umbilical icons with no extra libraries.

- src-tauri/icons/source.png     : 1024x1024 app icon, a white ring (feed it to `tauri icon`)
- src-tauri/icons/tray.png       : 64x64 black ring, a template icon for the macOS
  menu bar (macOS makes it black or white)
- src-tauri/icons/tray-white.png : 64x64 white ring for the Windows and Linux tray
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


def ring(x, y):
    """Distance to a ring in the middle, in unit space (-1..1)."""
    return abs(math.hypot(x, y) - 0.42) - 0.09


def draw(size, rgb, zoom=1.0):
    """Only the ring in `rgb`, on a clear background.
    zoom > 1 draws the ring bigger, so it fills more of the image."""
    px = [0] * (size * size * 4)
    aa = 2.0 / size / zoom
    for j in range(size):
        y = ((j + 0.5) / size * 2 - 1) / zoom
        for i in range(size):
            x = ((i + 0.5) / size * 2 - 1) / zoom
            g = cover(ring(x, y), aa)
            k = (j * size + i) * 4
            px[k:k + 4] = list(rgb) + [round(255 * g)]
    return px


WHITE = (255, 255, 255)
BLACK = (0, 0, 0)
OUT.mkdir(parents=True, exist_ok=True)
files = {
    "source.png": draw(1024, WHITE, zoom=1.8),
    "tray.png": draw(64, BLACK, zoom=1.8),
    "tray-white.png": draw(64, WHITE, zoom=1.8),
}
for name, px in files.items():
    size = int((len(px) // 4) ** 0.5)
    png(OUT / name, size, size, px)
    print("wrote", OUT / name)
