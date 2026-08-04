#!/usr/bin/env python3
"""Pixel- und Pfadmessungen für das Logo-Gate.

Getrennt vom Shell-Skript, weil Zusammenhangskomponenten und
Kontrastarithmetik in awk nur schwer nachvollziehbar wären.
"""
import re
import sys
from collections import deque

from PIL import Image

PATH_CMD = re.compile(r"[MmLlHhVvCcSsQqTtAaZz]")


def _alpha_mask(png):
    """True = Hintergrund (transparent), False = Marke."""
    img = Image.open(png).convert("RGBA")
    w, h = img.size
    a = img.getchannel("A").load()
    return w, h, [[a[x, y] < 128 for x in range(w)] for y in range(h)]


def bg_components(png, min_area=2):
    """Zahl der Hintergrund-Zusammenhangskomponenten, 4er-Nachbarschaft.

    Ein Klumpen ohne Aussparung hat genau 1: den Außenraum. Jede
    zusätzliche Komponente ist überlebender Negativraum.
    """
    w, h, bg = _alpha_mask(png)
    seen = [[False] * w for _ in range(h)]
    count = 0
    for sy in range(h):
        for sx in range(w):
            if seen[sy][sx] or not bg[sy][sx]:
                continue
            q = deque([(sx, sy)])
            seen[sy][sx] = True
            area = 0
            while q:
                x, y = q.popleft()
                area += 1
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < w and 0 <= ny < h and not seen[ny][nx] and bg[ny][nx]:
                        seen[ny][nx] = True
                        q.append((nx, ny))
            if area >= min_area:
                count += 1
    return count


def fill_ratio(png):
    """Anteil der Live-Fläche, den die Bounding-Box der Marke belegt."""
    w, h, bg = _alpha_mask(png)
    xs = [x for y in range(h) for x in range(w) if not bg[y][x]]
    ys = [y for y in range(h) for x in range(w) if not bg[y][x]]
    if not xs:
        return 0.0, 0.0
    return (max(xs) - min(xs) + 1) / w, (max(ys) - min(ys) + 1) / h


def path_stats(svg):
    """Zahl der Pfade und die höchste Befehlszahl eines einzelnen Pfades."""
    text = open(svg, encoding="utf-8").read()
    ds = re.findall(r'\sd\s*=\s*"([^"]*)"', text)
    if not ds:
        return 0, 0
    return len(ds), max(len(PATH_CMD.findall(d)) for d in ds)


def _luminance(hex_colour):
    hex_colour = hex_colour.lstrip("#")
    parts = [int(hex_colour[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    lin = [c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4 for c in parts]
    return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]


def contrast(fg, bg):
    a, b = _luminance(fg), _luminance(bg)
    hi, lo = max(a, b), min(a, b)
    return (hi + 0.05) / (lo + 0.05)


def main():
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == "bg-components":
        print(bg_components(args[0]))
    elif cmd == "fill-ratio":
        fw, fh = fill_ratio(args[0])
        print(f"{fw:.4f} {fh:.4f}")
    elif cmd == "path-stats":
        n, m = path_stats(args[0])
        print(f"{n} {m}")
    elif cmd == "contrast":
        print(f"{contrast(args[0], args[1]):.2f}")
    else:
        raise SystemExit(f"unknown command: {cmd}")


if __name__ == "__main__":
    main()
