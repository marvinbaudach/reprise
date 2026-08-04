#!/usr/bin/env python3
"""Vergibt SVG-`id`s eindeutig pro Datei.

`id` ist im Dokument eindeutig, nicht in der Datei. Sobald zwei Marken in
dieselbe HTML-Seite eingebettet werden — der Normalfall für eine Markenseite
mit heller und dunkler Fassung nebeneinander — gewinnt die zuerst geladene
Verlaufsdefinition und färbt die zweite Marke um. Deshalb bekommt jede
erzeugte Datei ihr eigenes Präfix.
"""
import re

_ID = re.compile(r'\bid="([^"]+)"')
_REF = re.compile(r'url\(#([^)]+)\)')
_HREF = re.compile(r'\b(xlink:href|href)="#([^"]+)"')


def prefix_ids(text, prefix):
    """Setzt `prefix` vor jede `id` und zieht alle Verweise mit."""
    names = set(_ID.findall(text))
    if not names:
        return text
    out = _ID.sub(lambda m: f'id="{prefix}{m.group(1)}"', text)
    out = _REF.sub(
        lambda m: f"url(#{prefix}{m.group(1)})" if m.group(1) in names else m.group(0),
        out)
    out = _HREF.sub(
        lambda m: (f'{m.group(1)}="#{prefix}{m.group(2)}"'
                   if m.group(2) in names else m.group(0)),
        out)
    return out


def inner(svg_text):
    """Der Inhalt zwischen dem Wurzel-`<svg>` und `</svg>`."""
    start = svg_text.index(">", svg_text.index("<svg")) + 1
    return svg_text[start:svg_text.rindex("</svg>")].strip("\n")


def view_box(svg_text):
    match = re.search(r'viewBox="([^"]+)"', svg_text)
    if not match:
        raise SystemExit("SVG ohne viewBox")
    return [float(v) for v in match.group(1).split()]
