#!/usr/bin/env python3
"""Render the maintained palette into the exact repeat-sign SVG sources."""

import argparse
import re
import tomllib
from pathlib import Path

PALETTE_KEYS = (
    "reprise_teal",
    "reprise_coral",
    "reprise_teal_light",
    "reprise_coral_light",
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


def hinted_16_body(small, large):
    """The repeat sign redrawn on the 16-unit grid, every edge on a whole line.

    The 96-unit geometry is the mark; this is the same sign at the one size
    where that geometry stops working. Scaled to 16px it renders four separate
    pixel groups of 20, 8, 2 and 2 pixels — nothing merges, but the dots are
    small enough that a detector treats them as noise and a viewer sees two
    specks. Here the dots are 3x3, the barlines are 1 and 3 pixels, and every
    gap is a whole pixel, so the 1:3 ratio that makes this a repeat sign rather
    than a rest survives at the smallest stage shipped.

    The dots are 3x3 rather than 2x2 because 2x2 leaves them at 4 pixels
    against a 3-pixel noise floor — countable, but one rounding decision away
    from vanishing. At 3x3 they are 9 pixels and read as dots beside a
    ten-pixel barline instead of as specks.

    Proportions run heavier than the 96-unit drawing on purpose: ink fills 64%
    of the carrier's width against 48% there, because thin features disappear
    at this size.
    """
    return (f'  <rect x="3" y="5" width="3" height="3" fill="{small}"/>\n'
            f'  <rect x="3" y="9" width="3" height="3" fill="{small}"/>\n'
            f'  <rect x="7" y="3" width="1" height="10" fill="{small}"/>\n'
            f'  <rect x="9" y="3" width="3" height="10" fill="{large}"/>\n')


def hinted_16_svg(small, large):
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16">\n'
            '  <!-- Generated from palette.toml. The repeat sign on the 16-unit\n'
            '       grid, for the one raster stage where the 96-unit geometry\n'
            '       renders the dots as two specks. -->\n'
            + hinted_16_body(small, large) + '</svg>\n')


def hinted_16_mono_svg():
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16"\n'
            '     fill="currentColor">\n'
            '  <!-- Single-colour form of the 16-unit drawing. -->\n'
            '  <rect x="3" y="5" width="3" height="3"/>\n'
            '  <rect x="3" y="9" width="3" height="3"/>\n'
            '  <rect x="7" y="3" width="1" height="10"/>\n'
            '  <rect x="9" y="3" width="3" height="10"/>\n'
            '</svg>\n')


def hinted_16_icon_svg(small, large, plate):
    """Carrier and sign in one file — at this size composing them would resample."""
    return ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16">\n'
            '  <!-- Generated from palette.toml. Carrier and sign are drawn\n'
            '       together because composing them at 16 units would land the\n'
            '       mark off the pixel grid it exists to sit on. -->\n'
            f'  <rect x="1" y="1" width="14" height="14" rx="4" fill="{plate}"/>\n'
            + hinted_16_body(small, large) + '</svg>\n')


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
    # The colour split was decided at dock size against the alternative that
    # swapped the two: teal on the thick barline reads farther because the
    # large field carries the brighter colour (7.13:1 on the plate against
    # coral's 4.42:1), and coral stays legible as the accent on the dots and
    # the thin barline. Only the chosen split is generated — a second one kept
    # "for comparison" is a second mark to keep in step, and the comparison is
    # over.
    return {
        "reprise-mark.svg": mark_svg(
            palette["reprise_coral"], palette["reprise_teal"],
            "Teal carries the thick barline so the\n"
            "       largest field holds the brighter colour; coral accents the dots\n"
            "       and the thin barline."),
        "reprise-mark-light.svg": mark_svg(
            palette["reprise_coral_light"], palette["reprise_teal_light"],
            "The deeper pair keeps that hierarchy\n"
            "       while clearing the graphical-object contrast floor on light ground."),
        "reprise-mark-mono.svg": mono_svg(),
        "icon-plate.svg": plate_svg(palette["reprise_plate"]),
        "reprise-mark-16.svg": hinted_16_svg(
            palette["reprise_coral"], palette["reprise_teal"]),
        "reprise-mark-16-mono.svg": hinted_16_mono_svg(),
        "reprise-icon-16.svg": hinted_16_icon_svg(
            palette["reprise_coral"], palette["reprise_teal"],
            palette["reprise_plate"]),
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
