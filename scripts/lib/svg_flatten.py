#!/usr/bin/env python3
"""Flacht ein SVG auf eine einzige Füllfarbe ab — die Grundlage von V3.

V3 fragt: trägt die Zeichnung auch ohne Farbe? Deshalb muss das Abflachen
jede Form gleich behandeln, egal wie ihre Farbe notiert ist. Die erste
Fassung ersetzte nur `fill="…"`-Attribute und war damit blind für
`style="fill:…"` (Inkscapes Exportform) und für Teiltransparenz.

Teiltransparenz ist der heikle Teil: eine Form mit `opacity="0.38"` landet
im Alphakanal unter der Schwelle des Negativraum-Tests und würde dort als
Aussparung gezählt. Das wäre erfundener Negativraum. Also wird jede
Deckkraft auf 1 gezogen.
"""
import re
import sys

COLOUR = "#000000"

# Farbwerte in Attributform. `none` bleibt `none` — eine nicht gefüllte Form
# soll auch monochrom nicht plötzlich Fläche tragen.
_ATTR_COLOUR = re.compile(
    r'\b(fill|stroke|stop-color|color|flood-color|lighting-color)="(?!none")[^"]*"')
_ATTR_OPACITY = re.compile(
    r'\b(opacity|fill-opacity|stroke-opacity|stop-opacity)="[^"]*"')
_STYLE = re.compile(r'\bstyle="([^"]*)"')
_STYLE_COLOUR = re.compile(
    r'\b(fill|stroke|stop-color|color|flood-color|lighting-color)\s*:\s*'
    r'(?!none)[^;]*')
_STYLE_OPACITY = re.compile(
    r'\b(opacity|fill-opacity|stroke-opacity|stop-opacity)\s*:\s*[^;]*')


def _flatten_style(match):
    body = match.group(1)
    body = _STYLE_COLOUR.sub(lambda m: f"{m.group(1)}:{COLOUR}", body)
    body = _STYLE_OPACITY.sub(lambda m: f"{m.group(1)}:1", body)
    return f'style="{body}"'


def flatten(text):
    out = _ATTR_COLOUR.sub(lambda m: f'{m.group(1)}="{COLOUR}"', text)
    out = _ATTR_OPACITY.sub(lambda m: f'{m.group(1)}="1"', out)
    out = _STYLE.sub(_flatten_style, out)
    return out


def main():
    source, destination = sys.argv[1], sys.argv[2]
    text = open(source, encoding="utf-8").read()
    flat = flatten(text)
    if flat == text:
        # Eine Zeichnung ohne eine einzige Farbangabe gibt es nicht. Wenn hier
        # nichts ersetzt wurde, hat der Filter die Notation nicht erkannt —
        # und ein stiller No-op würde V3 zum Placebo machen.
        raise SystemExit(f"V3: in {source} war keine Farbangabe zu ersetzen")
    open(destination, "w", encoding="utf-8").write(flat)


if __name__ == "__main__":
    main()
