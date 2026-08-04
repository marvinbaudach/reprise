#!/usr/bin/env python3
"""Setzt den Schriftzug — einmal als Pfade, einmal als eingebettete Schrift.

Beide Fassungen des Lockups müssen dasselbe zeigen. Die erste Fassung
rechnete die Pfadversion ohne Kerning und ließ die Live-Text-Version vom
Renderer kernen: derselbe Schriftzug, zwei Laufweiten. Ein Fallback, der
anders aussieht, ist kein Fallback. Deshalb liegt die Positionierung hier
an einer Stelle, und beide Fassungen holen sie sich von dort.

Die Schrift wird auf die tatsächlich benutzten Zeichen verkleinert und als
`@font-face` in die Live-Fassung eingebettet. Vorher lag die TTF im Baum,
ohne dass irgendetwas sie referenzierte — die Live-Fassung fiel auf Georgia
zurück, sobald Fraunces nicht im System installiert war.
"""
import base64
import io
import sys
from pathlib import Path

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont

# Der Wert stammt aus der GPOS-Tabelle und ist in Font-Einheiten notiert.
KERN_FEATURE = "kern"


def _kern_lookup_indices(gpos):
    indices = set()
    for record in gpos.FeatureList.FeatureRecord:
        if record.FeatureTag == KERN_FEATURE:
            indices.update(record.Feature.LookupListIndex)
    return indices


def kern_pairs(font):
    """Alle Paar-Kerningwerte als {(links, rechts): Wert}."""
    if "GPOS" not in font:
        return {}
    gpos = font["GPOS"].table
    pairs = {}
    for index in _kern_lookup_indices(gpos):
        lookup = gpos.LookupList.Lookup[index]
        for sub in lookup.SubTable:
            if getattr(sub, "Format", None) == 1:
                for first, pair_set in zip(sub.Coverage.glyphs, sub.PairSet):
                    for record in pair_set.PairValueRecord:
                        value = getattr(record.Value1, "XAdvance", 0)
                        if value:
                            pairs[(first, record.SecondGlyph)] = value
            elif getattr(sub, "Format", None) == 2:
                class1 = sub.ClassDef1.classDefs
                class2 = sub.ClassDef2.classDefs
                covered = set(sub.Coverage.glyphs)
                by_class1 = {}
                for glyph in covered:
                    by_class1.setdefault(class1.get(glyph, 0), []).append(glyph)
                by_class2 = {}
                for glyph, klass in class2.items():
                    by_class2.setdefault(klass, []).append(glyph)
                for k1, record1 in enumerate(sub.Class1Record):
                    for k2, record2 in enumerate(record1.Class2Record):
                        value = getattr(record2.Value1, "XAdvance", 0)
                        if not value:
                            continue
                        for first in by_class1.get(k1, ()):
                            for second in by_class2.get(k2, ()):
                                pairs[(first, second)] = value
    return pairs


def layout(font, text, size, tracking=0.0):
    """Glyphenpositionen in SVG-Einheiten, Kerning und Laufweite eingerechnet.

    Rückgabe: Liste aus (Glyphenname, x) und die Gesamtvorschubbreite.
    """
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    upem = font["head"].unitsPerEm
    scale = size / upem
    pairs = kern_pairs(font)

    placed, x = [], 0.0
    names = [cmap[ord(character)] for character in text]
    for index, name in enumerate(names):
        placed.append((name, x * scale + index * tracking))
        x += glyphs[name].width
        if index + 1 < len(names):
            x += pairs.get((name, names[index + 1]), 0)
    width = x * scale + max(0, len(names) - 1) * tracking
    return placed, width, scale


def outline(font, text, size, tracking=0.0, indent="  "):
    """Der Schriftzug als SVG-Pfade, relativ zur Grundlinie bei y=0."""
    glyphs = font.getGlyphSet()
    placed, width, scale = layout(font, text, size, tracking)
    parts = []
    for name, x in placed:
        pen = SVGPathPen(glyphs)
        glyphs[name].draw(pen)
        commands = pen.getCommands()
        if not commands:
            continue
        parts.append(f'{indent}<path transform="translate({x:.4f} 0) '
                     f'scale({scale:.6f} {-scale:.6f})" d="{commands}"/>')
    return "\n".join(parts), width


def font_face(ttf_path, text, family="Fraunces", weight=600):
    """`@font-face` mit der auf `text` verkleinerten Schrift als Data-URI."""
    from fontTools import subset

    # `recalcTimestamp=False`: sonst schreibt fontTools beim Speichern die
    # aktuelle Uhrzeit in `head.modified`, jede erzeugte Datei unterscheidet
    # sich von der vorigen, und die Prüfung „stammt diese Datei aus den
    # Zeichnungen?" schlägt bei jedem Bau an, ohne dass sich etwas geändert
    # hat. Das Feld selbst zu setzen genügt nicht — es wird überschrieben.
    font = TTFont(ttf_path, recalcTimestamp=False)
    options = subset.Options()
    options.layout_features = ["kern"]
    options.name_IDs = [1, 2, 3, 4, 6]
    options.notdef_outline = True
    subsetter = subset.Subsetter(options=options)
    subsetter.populate(text=text)
    subsetter.subset(font)
    buffer = io.BytesIO()
    font.save(buffer)
    data = base64.b64encode(buffer.getvalue()).decode("ascii")
    return (
        "  <style>\n"
        "    @font-face {\n"
        f"      font-family: '{family}';\n"
        f"      font-weight: {weight};\n"
        "      font-style: normal;\n"
        f"      src: url(data:font/ttf;base64,{data}) format('truetype');\n"
        "    }\n"
        "  </style>"
    )


def main():
    ttf, text, size = sys.argv[1], sys.argv[2], float(sys.argv[3])
    tracking = float(sys.argv[4]) if len(sys.argv) > 4 else 0.0
    paths, width = outline(TTFont(ttf), text, size, tracking)
    print(paths)
    print(f"<!-- Vorschubbreite: {width:.2f} -->", file=sys.stderr)


if __name__ == "__main__":
    main()
