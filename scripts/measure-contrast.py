#!/usr/bin/env python3
"""Measure WCAG text contrast in a screenshot region.

The popover contrast rules (`docs/ux-rules.md`, CONTRAST-3 and CONTRAST-5) are
verified by measuring real pixels rather than trusting the palette. Their
acceptance repeats the same measurement; green unit tests are only the
starting state.

Usage:
    measure-contrast.py SHOT.png X0 Y0 X1 Y1 [LABEL]
    measure-contrast.py SHOT.png --regions regions.tsv

The region is assumed to hold one text run on one flat surface. The surface is
taken as the median luminance pixel and the glyph core as the 99.5th
percentile, which survives antialiasing without picking up a stray highlight.
"""

import sys

try:
    from PIL import Image
except ImportError:
    sys.exit("needs pillow: pip install --user pillow")

AA_TEXT_MINIMUM = 4.5


def _to_linear(channel: float) -> float:
    channel /= 255.0
    if channel <= 0.04045:
        return channel / 12.92
    return ((channel + 0.055) / 1.055) ** 2.4


def luminance(pixel) -> float:
    red, green, blue = (_to_linear(c) for c in pixel)
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def contrast(foreground, background) -> float:
    first, second = luminance(foreground), luminance(background)
    lighter, darker = max(first, second), min(first, second)
    return (lighter + 0.05) / (darker + 0.05)


def measure(image, box):
    """Return (surface, glyph, ratio) for one text region."""
    x0, y0, x1, y1 = box
    raw = image.crop((x0, y0, x1, y1)).tobytes()
    pixels = [tuple(raw[index : index + 3]) for index in range(0, len(raw), 3)]
    if not pixels:
        raise ValueError(f"empty region {box}")
    ordered = sorted(pixels, key=luminance)
    surface = ordered[len(ordered) // 2]
    glyph = ordered[int(len(ordered) * 0.995)]
    # Light themes put the glyph below the surface; take whichever extreme
    # actually differs from the median.
    darkest = ordered[int(len(ordered) * 0.005)]
    if contrast(darkest, surface) > contrast(glyph, surface):
        glyph = darkest
    return surface, glyph, contrast(glyph, surface)


def _report(label, surface, glyph, ratio) -> bool:
    passed = ratio >= AA_TEXT_MINIMUM
    mark = "ok  " if passed else "FAIL"
    print(
        f"{mark} {label:32s} surface=#{surface[0]:02x}{surface[1]:02x}{surface[2]:02x}"
        f" text=#{glyph[0]:02x}{glyph[1]:02x}{glyph[2]:02x} ratio={ratio:5.2f}"
    )
    return passed


def main(argv) -> int:
    if len(argv) < 2:
        return int(bool(sys.exit(__doc__)))
    image = Image.open(argv[1]).convert("RGB")

    if len(argv) > 2 and argv[2] == "--regions":
        # TSV: label<TAB>x0<TAB>y0<TAB>x1<TAB>y1
        failures = 0
        with open(argv[3], encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if not line or line.startswith("#"):
                    continue
                label, *bounds = line.split("\t")
                surface, glyph, ratio = measure(image, [int(v) for v in bounds])
                failures += not _report(label, surface, glyph, ratio)
        return 1 if failures else 0

    box = [int(v) for v in argv[2:6]]
    label = argv[6] if len(argv) > 6 else "region"
    surface, glyph, ratio = measure(image, box)
    return 0 if _report(label, surface, glyph, ratio) else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
