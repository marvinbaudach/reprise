#!/usr/bin/env python3
"""Übersetzt eine Zeichnung in einen Android VectorDrawable.

Nötig, weil VectorDrawable nur <path> kennt: Ellipsen und Kreise müssen
vorher aufgelöst werden.

Die Skalierung richtet sich nach Androids **garantierter** Fläche. Das ist
nicht das 72-dp-Quadrat, sondern der 66-dp-Kreis um die Mitte: nur er ist
auf jeder Maskenform sichtbar. Die erste Fassung rechnete gegen das Quadrat
und ließ die Ohrlappen knapp 5 dp darüber hinausragen — auf Launchern mit
Kreismaske wurden sie flach abgeschnitten. Der Radius wird deshalb hier
gemessen, nicht geschätzt.
"""
import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from logo_measure import ink_box, radius                # noqa: E402
from svg_ids import view_box                            # noqa: E402

VIEWPORT = 108.0
SAFE_RADIUS = 33.0          # halber Durchmesser der garantierten Kreisfläche
PROBE_WIDTH = 512

# SVG-Zahlengrammatik inklusive Exponent. Der erste Wurf war `-?[\d.]+`:
# der zerlegt `1e-5` still in `1` und `-5` und verschluckt sich an `1.5.5`.
NUMBER = re.compile(r"[-+]?(?:\d*\.\d+|\d+\.?)(?:[eE][-+]?\d+)?")
COMMAND_ARITY = {"M": 2, "L": 2, "H": 1, "V": 1, "C": 6, "S": 4,
                 "Q": 4, "T": 2, "A": 7, "Z": 0}
TOKEN = re.compile(r"[MLHVCSQTAZz]|" + NUMBER.pattern)
SHAPE = re.compile(r"<(path|ellipse|circle)\b([^>]*?)/?>", re.S)
GROUP_FILL = re.compile(r'<(?:g|svg)\b[^>]*?\bfill="([^"]+)"')


class Transform:
    """Quellkoordinaten → 108er Raster, zentriert und auf den Kreis gepasst."""

    def __init__(self, scale, tx, ty):
        self.scale, self.tx, self.ty = scale, tx, ty

    def x(self, value):
        return value * self.scale + self.tx

    def y(self, value):
        return value * self.scale + self.ty


def measure(svg_path):
    """Mittelpunkt und größter Radius der gezeichneten Fläche, in Quelleinheiten."""
    _, _, vw, vh = view_box(Path(svg_path).read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory() as tmp:
        png = Path(tmp) / "probe.png"
        height = max(1, round(PROBE_WIDTH * vh / vw))
        subprocess.run(
            ["rsvg-convert", "-w", str(PROBE_WIDTH), "-h", str(height),
             str(svg_path), "-o", str(png)],
            check=True, capture_output=True)
        fx0, fy0, fx1, fy1 = ink_box(png)
        cx_share, cy_share = (fx0 + fx1) / 2, (fy0 + fy1) / 2
        # radius() gibt den Abstand in Anteilen der Bildbreite zurück. Da der
        # Render das Seitenverhältnis hält, ist ein Anteil der Breite direkt
        # ein Anteil der viewBox-Breite.
        r_share = radius(png, cx_share, cy_share)
    return cx_share * vw, cy_share * vh, r_share * vw


def build_transform(svg_path):
    cx, cy, r = measure(svg_path)
    scale = SAFE_RADIUS / r
    return Transform(scale, VIEWPORT / 2 - cx * scale, VIEWPORT / 2 - cy * scale)


def scale_path(d, transform):
    """Skaliert einen Pfad. Nur absolute Befehle.

    Relative Befehle werden abgelehnt statt still falsch umgerechnet: ihre
    Zahlen sind Abstände, keine Koordinaten, und eine Verschiebung darf sie
    nicht anfassen.

    Der Bogen ist der Sonderfall. Seine sieben Zahlen sind nicht abwechselnd
    x und y — die ersten beiden sind Radien, dann folgen Drehwinkel und zwei
    Flags. Paarweise umgerechnet käme ein anderer Bogen heraus. Da die
    Abbildung gleichmäßig skaliert, gehen die Radien mit demselben Faktor
    mit, Winkel und Flags bleiben.
    """
    if re.search(r"[mlhvcsqta]", d):
        raise SystemExit(f"nur absolute Befehle erlaubt, gefunden in: {d[:60]}…")
    tokens = TOKEN.findall(d)
    if len("".join(tokens).replace(" ", "")) < len(re.sub(r"[\s,]", "", d)):
        raise SystemExit(f"Pfad nicht vollständig erkannt: {d[:60]}…")
    out, pending, axis = [], None, 0
    for token in tokens:
        if token == "z":
            token = "Z"
        if token in COMMAND_ARITY:
            out.append(token)
            pending, axis = token, 0
            continue
        value = float(token)
        if pending == "A" and axis % 7 in (0, 1):
            out.append(f"{value * transform.scale:.3f}")
        elif pending == "A" and axis % 7 in (2, 3, 4):
            out.append(f"{value:g}")
        elif pending == "H":
            out.append(f"{transform.x(value):.3f}")
        elif pending == "V":
            out.append(f"{transform.y(value):.3f}")
        elif pending == "A":
            out.append(f"{(transform.x if axis % 7 == 5 else transform.y)(value):.3f}")
        else:
            out.append(f"{(transform.x if axis % 2 == 0 else transform.y)(value):.3f}")
        axis += 1
    return " ".join(out)


def ellipse_path(cx, cy, rx, ry, transform):
    cx, cy = transform.x(cx), transform.y(cy)
    rx, ry = rx * transform.scale, ry * transform.scale
    return (f"M{cx - rx:.3f},{cy:.3f}"
            f"a{rx:.3f},{ry:.3f} 0 1,0 {2 * rx:.3f},0"
            f"a{rx:.3f},{ry:.3f} 0 1,0 {-2 * rx:.3f},0z")


def resolve_fill(attrs, inherited):
    """Füllfarbe einer Form — Attribut, `style`, oder geerbt.

    Ein stiller Rückfall auf Schwarz hat den ausgelieferten Foreground-Layer
    getroffen: eine per `style="fill:…"` gesetzte Farbe verschwand spurlos.
    """
    match = re.search(r'fill="([^"]+)"', attrs)
    if match:
        return match.group(1)
    match = re.search(r'style="[^"]*\bfill\s*:\s*([^;"]+)', attrs)
    if match:
        return match.group(1).strip()
    if inherited:
        return inherited
    raise SystemExit(f"Form ohne erkennbare Füllung: {attrs.strip()[:80]}")


def check_flat(fill, attrs):
    """VectorDrawable kennt `url(#…)` nicht.

    Ein Verweis auf eine Verlaufsdefinition landete bisher wörtlich in
    `android:fillColor`. Android rendert das als Schwarz — die Zeichnung
    kompiliert, sieht aber anders aus als die Quelle. Deshalb hier hart
    abbrechen: für Android wird die flache Stufe gezeichnet, nicht die
    verlaufsreiche.
    """
    if fill.startswith("url("):
        raise SystemExit(
            f"Verlaufsfüllung {fill} kann kein VectorDrawable werden: "
            f"{attrs.strip()[:60]}")
    return fill


def convert(src, mono):
    text = Path(src).read_text(encoding="utf-8")
    transform = build_transform(src)
    inherited = GROUP_FILL.search(text)
    inherited = inherited.group(1) if inherited else None
    shapes = []
    for match in SHAPE.finditer(text):
        kind, attrs = match.group(1), match.group(2)
        fill = "#000000" if mono else check_flat(resolve_fill(attrs, inherited),
                                                 attrs)
        if fill == "none":
            continue
        if fill == "currentColor":
            fill = "#000000"
        fill_type = "evenOdd" if "evenodd" in attrs else "nonZero"
        if kind == "path":
            d = re.search(r'\sd="([^"]+)"', attrs, re.S).group(1)
            shapes.append((scale_path(d, transform), fill, fill_type))
        else:
            def number(key):
                found = re.search(rf'{key}="([^"]+)"', attrs)
                if not found:
                    raise SystemExit(f"{kind} ohne {key}")
                return float(found.group(1))
            rx = number("r") if kind == "circle" else number("rx")
            ry = rx if kind == "circle" else number("ry")
            shapes.append((ellipse_path(number("cx"), number("cy"), rx, ry,
                                        transform), fill, fill_type))
    if not shapes:
        raise SystemExit(f"keine Formen in {src}")

    head = ('<?xml version="1.0" encoding="utf-8"?>\n'
            "<!-- Erzeugt von scripts/build-brand-assets.sh."
            " Nicht von Hand ändern. -->\n"
            '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
            '    android:width="108dp"\n'
            '    android:height="108dp"\n'
            '    android:viewportWidth="108"\n'
            '    android:viewportHeight="108">\n')
    body = "".join(
        f'    <path android:fillColor="{fill}" android:fillType="{fill_type}"\n'
        f'          android:pathData="{d}"/>\n'
        for d, fill, fill_type in shapes)
    return head + body + "</vector>\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source")
    parser.add_argument("destination")
    parser.add_argument("--mono", action="store_true",
                        help="alle Flächen schwarz: das System tönt den Layer")
    args = parser.parse_args()
    Path(args.destination).write_text(convert(args.source, args.mono),
                                      encoding="utf-8")


if __name__ == "__main__":
    main()
