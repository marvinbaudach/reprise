#!/usr/bin/env python3
"""Render deterministic square and circular legacy Android launcher PNGs."""

import argparse
import subprocess
import tempfile
from pathlib import Path

from PIL import Image

SIZES = (48, 72, 96, 144, 192)
DENSITIES = ("mdpi", "hdpi", "xhdpi", "xxhdpi", "xxxhdpi")


def render(source, size, destination):
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        raw = Path(tmp) / "icon.png"
        subprocess.run(
            ["rsvg-convert", "-w", str(size), "-h", str(size),
             str(source), "-o", str(raw)],
            check=True,
            capture_output=True,
        )
        image = Image.open(raw).convert("RGBA")
        image.save(destination, optimize=True)


def build(source, res_dir):
    root = Path(res_dir)
    for density, size in zip(DENSITIES, SIZES, strict=True):
        target = root / f"mipmap-{density}"
        render(source, size, target / "ic_launcher.png")
        render(source, size, target / "ic_launcher_round.png")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source")
    parser.add_argument("res_dir")
    args = parser.parse_args()
    build(Path(args.source), args.res_dir)


if __name__ == "__main__":
    main()
