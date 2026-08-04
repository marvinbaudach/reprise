#!/usr/bin/env python3
"""Setzt einen kurzen Schriftzug als SVG-Pfaddaten.

Wird für die Outlined-Lockups gebraucht: Live-Text bricht ohne geladene
Schrift, die Pfadfassung nicht.
"""
import sys

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont


def wordmark(ttf_path, text, size):
    font = TTFont(ttf_path)
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    upem = font["head"].unitsPerEm
    scale = size / upem
    parts, x = [], 0.0
    for character in text:
        name = cmap[ord(character)]
        pen = SVGPathPen(glyphs)
        glyphs[name].draw(pen)
        d = pen.getCommands()
        if d:
            parts.append(f'<path transform="translate({x * scale:.3f} 0) '
                         f'scale({scale:.6f} {-scale:.6f})" d="{d}"/>')
        x += glyphs[name].width
    return "\n".join(parts), x * scale


if __name__ == "__main__":
    paths, width = wordmark(sys.argv[1], sys.argv[2], float(sys.argv[3]))
    print(paths)
    print(f"<!-- Vorschubbreite: {width:.2f} -->", file=sys.stderr)
