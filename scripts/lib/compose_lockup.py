#!/usr/bin/env python3
"""Erzeugt ein Lockup in zwei Fassungen: mit Live-Text und mit Pfaden.

Beide Fassungen entstehen aus **einer** Positionsrechnung. Vorher wurden
Schriftgröße, Grundlinie und Laufweite in zwei Dateien getrennt gepflegt,
und die Pfadfassung saß anders als die Live-Fassung — ein Fallback, der
anders aussieht als das Original.
"""
import argparse
import sys
from pathlib import Path

from fontTools.ttLib import TTFont

sys.path.insert(0, str(Path(__file__).resolve().parent))
from compose_icon import ink_bounds                     # noqa: E402
from svg_ids import inner, prefix_ids, view_box         # noqa: E402
from wordmark import font_face, layout, outline         # noqa: E402

PAD = 4.0
GAP_HORIZONTAL = 0.24      # Anteil der Markenhöhe
GAP_VERTICAL = 0.20
GENERATED = ("  <!-- Erzeugt von scripts/build-brand-assets.sh. "
             "Nicht von Hand ändern. -->")


def cap_height(font, size):
    os2 = font["OS/2"]
    units = getattr(os2, "sCapHeight", 0) or font["head"].unitsPerEm * 0.7
    return units * size / font["head"].unitsPerEm


def _mark_block(mark_path, prefix, scale, tx, ty):
    text = Path(mark_path).read_text(encoding="utf-8")
    body = prefix_ids(inner(text), prefix)
    return (f'  <g id="{prefix}mark" transform="translate({tx:.4f} {ty:.4f}) '
            f'scale({scale:.6f})">\n{body}\n  </g>')


def build(mark_path, ttf_path, text, mode, size, tracking, mark_height):
    font = TTFont(ttf_path)
    _, advance, _ = layout(font, text, size, tracking)
    caps = cap_height(font, size)

    vb = view_box(Path(mark_path).read_text(encoding="utf-8"))
    x0, y0, x1, y1 = ink_bounds(mark_path, vb)
    scale = mark_height / (y1 - y0)
    mark_width = scale * (x1 - x0)

    if mode == "horizontal":
        gap = GAP_HORIZONTAL * mark_height
        width = PAD * 2 + mark_width + gap + advance
        height = PAD * 2 + mark_height
        mark_tx = PAD - scale * x0
        mark_ty = PAD - scale * y0
        text_x = PAD + mark_width + gap
        baseline = PAD + mark_height / 2 + caps / 2
        anchor = ""
    else:
        gap = GAP_VERTICAL * mark_height
        width = PAD * 2 + max(mark_width, advance)
        height = PAD * 2 + mark_height + gap + caps
        mark_tx = (width - mark_width) / 2 - scale * x0
        mark_ty = PAD - scale * y0
        text_x = width / 2
        baseline = PAD + mark_height + gap + caps
        anchor = ' text-anchor="middle"'

    return {
        "width": width,
        "height": height,
        "mark": (scale, mark_tx, mark_ty),
        "text": (text_x, baseline, anchor),
        "advance": advance,
        "font": font,
    }


def render(plan, mark_path, ttf_path, text, size, tracking, prefix, outlined):
    width, height = plan["width"], plan["height"]
    scale, mark_tx, mark_ty = plan["mark"]
    text_x, baseline, anchor = plan["text"]
    head = (f'<svg xmlns="http://www.w3.org/2000/svg" '
            f'viewBox="0 0 {width:.2f} {height:.2f}" '
            f'width="{width:.2f}" height="{height:.2f}">\n{GENERATED}\n')
    body = _mark_block(mark_path, prefix, scale, mark_tx, mark_ty)
    if outlined:
        paths, _ = outline(plan["font"], text, size, tracking, indent="    ")
        word = (f'  <g fill="currentColor" '
                f'transform="translate({text_x:.4f} {baseline:.4f})">\n'
                f"{paths}\n  </g>")
        if anchor:
            word = word.replace(f"translate({text_x:.4f}",
                                f"translate({text_x - plan['advance'] / 2:.4f}")
    else:
        face = font_face(ttf_path, text)
        word = (f"{face}\n"
                f'  <text x="{text_x:.4f}" y="{baseline:.4f}"{anchor}\n'
                f"        font-family=\"Fraunces, 'Instrument Serif', Georgia, serif\"\n"
                f'        font-size="{size:g}" font-weight="600" '
                f'letter-spacing="{tracking:g}"\n'
                f'        fill="currentColor">{text}</text>')
    return f"{head}{body}\n{word}\n</svg>\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mark")
    parser.add_argument("ttf")
    parser.add_argument("--text", default="Reprise")
    parser.add_argument("--mode", choices=("horizontal", "vertical"),
                        required=True)
    parser.add_argument("--size", type=float, required=True)
    parser.add_argument("--tracking", type=float, default=0.0)
    parser.add_argument("--mark-height", type=float, required=True)
    parser.add_argument("--prefix", required=True)
    parser.add_argument("--live", required=True)
    parser.add_argument("--outlined", required=True)
    args = parser.parse_args()

    plan = build(args.mark, args.ttf, args.text, args.mode, args.size,
                 args.tracking, args.mark_height)
    Path(args.live).write_text(
        render(plan, args.mark, args.ttf, args.text, args.size, args.tracking,
               args.prefix, outlined=False), encoding="utf-8")
    Path(args.outlined).write_text(
        render(plan, args.mark, args.ttf, args.text, args.size, args.tracking,
               f"{args.prefix}o-", outlined=True), encoding="utf-8")


if __name__ == "__main__":
    main()
