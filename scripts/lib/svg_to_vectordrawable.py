#!/usr/bin/env python3
"""Übersetzt die reduzierte Zeichnung in einen Android VectorDrawable.

Nötig, weil VectorDrawable nur <path> kennt: Ellipsen, Kreise und
fill-rule müssen vorher aufgelöst werden. Die Skalierung setzt die Marke
in die 72dp-Safe-Zone des 108dp-Viewports.
"""
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

SCALE = 72 / 64
OFFSET = (108 - 72) / 2


def ellipse_path(cx, cy, rx, ry):
    cx, cy = cx * SCALE + OFFSET, cy * SCALE + OFFSET
    rx, ry = rx * SCALE, ry * SCALE
    return (f"M{cx - rx:.3f},{cy:.3f}"
            f"a{rx:.3f},{ry:.3f} 0 1,0 {2 * rx:.3f},0"
            f"a{rx:.3f},{ry:.3f} 0 1,0 {-2 * rx:.3f},0z")


def scale_path(d):
    """Skaliert einen Pfad aufs 108er Raster. Nur absolute M/L/C/Z.

    Beide Achsen bekommen dieselbe Transformation, deshalb genügt es, jede
    Zahl gleich zu behandeln. Andere Befehle werden abgelehnt statt still
    falsch umgerechnet — bei einem Bogen wären Radien und Flags keine
    Koordinaten.
    """
    if re.search(r"[mlhvcsqtaHVSQTA]", d):
        raise SystemExit("nur absolute M/L/C/Z erlaubt: mark-reduced.svg anpassen")
    out = []
    for token in re.findall(r"[MLCZz]|-?[\d.]+", d):
        if token in "MLCZz":
            out.append(token.upper())
        else:
            out.append(f"{float(token) * SCALE + OFFSET:.3f}")
    return " ".join(out)


def union_path(src):
    """Vereinigt die sichtbaren Pfade, ohne überlappende Flächen auszustanzen."""
    with tempfile.TemporaryDirectory() as tmp:
        destination = Path(tmp) / "union.svg"
        env = os.environ.copy()
        env["GSETTINGS_BACKEND"] = "memory"
        env["XDG_CONFIG_HOME"] = str(Path(tmp) / "config")
        actions = ("select-all;path-union;export-plain-svg;"
                   f"export-filename:{destination};export-do")
        subprocess.run(
            ["inkscape", f"--actions={actions}", src],
            check=True,
            capture_output=True,
            text=True,
            env=env,
        )
        text = destination.read_text(encoding="utf-8")
        match = re.search(r'<path\b[^>]*\sd="([^"]+)"', text)
        if not match:
            raise SystemExit("Inkscape lieferte keinen vereinigten Pfad")
        return scale_path(match.group(1))


def convert(src, mono):
    text = open(src, encoding="utf-8").read()
    shapes = []
    for match in re.finditer(r"<(path|ellipse|circle)\b([^>]*)/?>", text):
        kind, attrs = match.group(1), match.group(2)
        fill = (re.search(r'fill="([^"]+)"', attrs) or [None, "#000000"])[1]
        fill_type = "evenOdd" if 'fill-rule="evenodd"' in attrs else "nonZero"
        if kind == "path":
            d = re.search(r'\sd="([^"]+)"', attrs).group(1)
            shapes.append((scale_path(d), fill, fill_type))
        else:
            get = lambda key, default=None: float(
                (re.search(rf'{key}="([^"]+)"', attrs) or [None, default])[1])
            if kind == "circle":
                radius = get("r")
                shapes.append((ellipse_path(get("cx"), get("cy"), radius, radius),
                               fill, fill_type))
            else:
                shapes.append((ellipse_path(get("cx"), get("cy"), get("rx"), get("ry")),
                               fill, fill_type))

    head = ('<?xml version="1.0" encoding="utf-8"?>\n'
            '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
            '    android:width="108dp"\n'
            '    android:height="108dp"\n'
            '    android:viewportWidth="108"\n'
            '    android:viewportHeight="108">\n')
    if mono:
        # Eine Fläche, Augen als Löcher. Das System tönt den Layer; nur Alpha zählt.
        merged = union_path(src)
        body = ('    <path android:fillColor="#000000" android:fillType="nonZero"\n'
                f'          android:pathData="{merged}"/>\n')
    else:
        body = "".join(
            f'    <path android:fillColor="{fill}" android:fillType="{fill_type}" '
            f'android:pathData="{d}"/>\n'
            for d, fill, fill_type in shapes)
    return head + body + "</vector>\n"


if __name__ == "__main__":
    source, destination = sys.argv[1], sys.argv[2]
    open(destination, "w", encoding="utf-8").write(
        convert(source, mono="monochrome" in destination))
    print("geschrieben", destination)
