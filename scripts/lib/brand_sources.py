#!/usr/bin/env python3
"""Render the maintained palette into the exact repeat-sign SVG sources."""

import argparse
import re
import tomllib
from pathlib import Path

PALETTE_KEYS = (
    "reprise_teal",
    "reprise_violet",
    "reprise_teal_light",
    "reprise_violet_light",
    "reprise_plate",
)
HEX = re.compile(r"#[0-9A-F]{6}")


def read_palette(path):
    palette = tomllib.loads(Path(path).read_text(encoding="utf-8"))
    if tuple(palette) != PALETTE_KEYS:
        raise SystemExit(
            f"palette keys must be exactly {', '.join(PALETTE_KEYS)}")
    invalid = [key for key, value in palette.items()
               if not isinstance(value, str) or not HEX.fullmatch(value)]
    if invalid:
        raise SystemExit(f"invalid uppercase six-digit palette values: {invalid}")
    if len(set(palette.values())) != len(palette):
        raise SystemExit("palette values must be unique")
    return palette


def mark_svg(small, large, explanation):
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96" width="96" height="96">
  <!-- Generated from palette.toml. {explanation} -->
  <circle cx="30" cy="39" r="5.5" fill="{small}"/>
  <circle cx="30" cy="57" r="5.5" fill="{small}"/>
  <rect x="41" y="20" width="5" height="56" rx="1" fill="{small}"/>
  <rect x="52" y="20" width="15" height="56" rx="1.5" fill="{large}"/>
</svg>
'''


def mono_svg():
    return '''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96" width="96" height="96"
     fill="currentColor">
  <!-- One geometry serves every theme-controlled surface so the symbolic
       and coloured repeat signs cannot drift into different marks. -->
  <circle cx="30" cy="39" r="5.5"/>
  <circle cx="30" cy="57" r="5.5"/>
  <rect x="41" y="20" width="5" height="56" rx="1"/>
  <rect x="52" y="20" width="15" height="56" rx="1.5"/>
</svg>
'''


def plate_svg(fill):
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96" width="96" height="96">
  <!-- Generated from palette.toml. The solid plate is deliberately lighter
       than the dock so the icon keeps one visible silhouette around the sign. -->
  <g id="rp-plate">
    <rect x="4" y="4" width="88" height="88" rx="22" fill="{fill}"/>
  </g>
</svg>
'''


def render(palette):
    teal = palette["reprise_teal"]
    violet = palette["reprise_violet"]
    teal_light = palette["reprise_teal_light"]
    violet_light = palette["reprise_violet_light"]
    return {
        "reprise-mark-a.svg": mark_svg(
            violet, teal,
            "Variant A puts violet on the larger\n       barline so the accent carries farther at dock size."),
        "reprise-mark-b.svg": mark_svg(
            teal, violet,
            "Variant B keeps the smaller dots and\n       thin barline teal so their distinction survives longer when rasterised."),
        "reprise-mark-a-light.svg": mark_svg(
            violet_light, teal_light,
            "The deeper colours preserve variant A's\n       hierarchy while meeting the graphical-object contrast floor on light ground."),
        "reprise-mark-b-light.svg": mark_svg(
            teal_light, violet_light,
            "The deeper colours preserve variant B's\n       small-element emphasis without losing contrast on light ground."),
        "reprise-mark-mono.svg": mono_svg(),
        "icon-plate.svg": plate_svg(palette["reprise_plate"]),
    }


def write_sources(palette_path, output_dir):
    output = Path(output_dir)
    output.mkdir(parents=True, exist_ok=True)
    for name, text in render(read_palette(palette_path)).items():
        (output / name).write_text(text, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("palette")
    parser.add_argument("output_dir")
    args = parser.parse_args()
    write_sources(args.palette, args.output_dir)


if __name__ == "__main__":
    main()
