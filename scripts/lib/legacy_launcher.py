#!/usr/bin/env python3
"""Render deterministic square and circular legacy Android launcher PNGs."""

import argparse
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

SIZES = (48, 72, 96, 144, 192)
DENSITIES = ("mdpi", "hdpi", "xhdpi", "xxhdpi", "xxxhdpi")
SUPERSAMPLE = 4


def render(source, size, destination, circular):
    destination.parent.mkdir(parents=True, exist_ok=True)
    render_size = size * SUPERSAMPLE if circular else size
    with tempfile.TemporaryDirectory() as tmp:
        raw = Path(tmp) / "icon.png"
        subprocess.run(
            ["rsvg-convert", "-w", str(render_size), "-h", str(render_size),
             str(source), "-o", str(raw)],
            check=True,
            capture_output=True,
        )
        image = Image.open(raw).convert("RGBA")
        if circular:
            mask = Image.new("L", (render_size, render_size), 0)
            ImageDraw.Draw(mask).ellipse((0, 0, render_size - 1, render_size - 1), fill=255)
            image.putalpha(mask)
            image = image.resize((size, size), Image.Resampling.LANCZOS)
        image.save(destination, optimize=True)


def build(source, res_dir):
    root = Path(res_dir)
    for density, size in zip(DENSITIES, SIZES, strict=True):
        target = root / f"mipmap-{density}"
        render(source, size, target / "ic_launcher.png", circular=False)
        render(source, size, target / "ic_launcher_round.png", circular=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source")
    parser.add_argument("res_dir")
    args = parser.parse_args()
    build(Path(args.source), args.res_dir)


if __name__ == "__main__":
    main()
