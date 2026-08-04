#!/usr/bin/env python3
"""Erzeugt Androids Hintergrundebene aus der Platte des App-Icons.

Der Verlauf steht genau einmal im Baum, in `data/brand/icon-plate.svg`.
Ihn für Android abzutippen hieße, ihn zweimal zu pflegen — und die
Android-Fassung würde als erste veralten, weil sie am seltensten
angeschaut wird.

Die Hintergrundebene ist randlos: Androids Maske schneidet die Ecken,
eine eigene Rundung darunter erzeugte nur einen dunklen Saum.
"""
import argparse
import re
from pathlib import Path

VIEWPORT = 108.0

_GRADIENT = re.compile(r"<linearGradient\b([^>]*)>(.*?)</linearGradient>", re.S)
_STOP = re.compile(r"<stop\b[^>]*offset=\"([^\"]+)\"[^>]*stop-color=\"([^\"]+)\"")


def _attr(text, name, default):
    match = re.search(rf'\b{name}="([^"]+)"', text)
    return float(match.group(1)) if match else default


def gradient(plate_text):
    """Start-, Endpunkt und Farbmarken des Verlaufs, in Viewport-Einheiten."""
    found = _GRADIENT.search(plate_text)
    if not found:
        raise SystemExit("Platte ohne linearGradient")
    head, body = found.group(1), found.group(2)
    stops = [(float(offset), colour) for offset, colour in _STOP.findall(body)]
    if len(stops) < 2:
        raise SystemExit(f"Verlauf mit {len(stops)} Farbmarke(n), mindestens 2 nötig")
    # objectBoundingBox ist der Vorgabewert von SVG und der Fall, den die
    # Platte benutzt: die Koordinaten sind Anteile der Fläche.
    units = re.search(r'gradientUnits="([^"]+)"', head)
    if units and units.group(1) != "objectBoundingBox":
        raise SystemExit(f"gradientUnits={units.group(1)} wird nicht abgebildet")
    box = [_attr(head, "x1", 0.0), _attr(head, "y1", 0.0),
           _attr(head, "x2", 1.0), _attr(head, "y2", 0.0)]
    return [value * VIEWPORT for value in box], stops


def build(plate_text):
    (x1, y1, x2, y2), stops = gradient(plate_text)
    items = "".join(
        f'                    <item android:offset="{offset:g}" '
        f'android:color="{colour}"/>\n'
        for offset, colour in stops)
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        "<!-- Erzeugt von scripts/build-brand-assets.sh."
        " Nicht von Hand ändern. -->\n"
        '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
        '    xmlns:aapt="http://schemas.android.com/aapt"\n'
        f'    android:width="{VIEWPORT:g}dp"\n'
        f'    android:height="{VIEWPORT:g}dp"\n'
        f'    android:viewportWidth="{VIEWPORT:g}"\n'
        f'    android:viewportHeight="{VIEWPORT:g}">\n'
        f'    <path android:pathData="M0,0h{VIEWPORT:g}v{VIEWPORT:g}h-{VIEWPORT:g}z">\n'
        '        <aapt:attr name="android:fillColor">\n'
        '            <gradient android:type="linear"\n'
        f'                android:startX="{x1:g}" android:startY="{y1:g}"\n'
        f'                android:endX="{x2:g}" android:endY="{y2:g}">\n'
        f"{items}"
        "            </gradient>\n"
        "        </aapt:attr>\n"
        "    </path>\n"
        "</vector>\n"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plate")
    parser.add_argument("destination")
    args = parser.parse_args()
    Path(args.destination).write_text(
        build(Path(args.plate).read_text(encoding="utf-8")), encoding="utf-8")


if __name__ == "__main__":
    main()
