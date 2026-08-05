#!/usr/bin/env python3
"""Translate an SVG drawing into an Android VectorDrawable.

VectorDrawable only carries paths, so circles, ellipses and rounded rectangles
are resolved here. Drawings may either be fitted to Android's guaranteed
66-dp circle or placed at a fixed offset without scaling. Literal SVG colours
can be mapped to Android resources so generated XML does not become another
maintained palette.
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
SHAPE = re.compile(r"<(path|ellipse|circle|rect)\b([^>]*?)/?>", re.S)
GROUP_FILL = re.compile(r'<(?:g|svg)\b[^>]*?\bfill="([^"]+)"')


class Transform:
    """Map source coordinates onto Android's 108-unit viewport."""

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


def fixed_transform(svg_path, offset):
    x, y, width, height = view_box(Path(svg_path).read_text(encoding="utf-8"))
    if x != 0 or y != 0:
        raise SystemExit(f"fixed placement needs a zero-origin viewBox, got {x:g} {y:g}")
    if width + 2 * offset > VIEWPORT or height + 2 * offset > VIEWPORT:
        raise SystemExit(
            f"{width:g}×{height:g} source with offset {offset:g} exceeds 108 viewport")
    return Transform(1.0, offset, offset)


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


def rounded_rect_path(x, y, width, height, rx, ry, transform):
    x0, y0 = transform.x(x), transform.y(y)
    x1, y1 = transform.x(x + width), transform.y(y + height)
    rx, ry = rx * transform.scale, ry * transform.scale
    if rx == 0 or ry == 0:
        return f"M{x0:.3f},{y0:.3f}H{x1:.3f}V{y1:.3f}H{x0:.3f}Z"
    return (
        f"M{x0 + rx:.3f},{y0:.3f}H{x1 - rx:.3f}"
        f"A{rx:.3f},{ry:.3f} 0 0,1 {x1:.3f},{y0 + ry:.3f}"
        f"V{y1 - ry:.3f}A{rx:.3f},{ry:.3f} 0 0,1 {x1 - rx:.3f},{y1:.3f}"
        f"H{x0 + rx:.3f}A{rx:.3f},{ry:.3f} 0 0,1 {x0:.3f},{y1 - ry:.3f}"
        f"V{y0 + ry:.3f}A{rx:.3f},{ry:.3f} 0 0,1 {x0 + rx:.3f},{y0:.3f}Z"
    )


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


GRADIENT = re.compile(
    r'<linearGradient\b([^>]*\bid="([^"]+)"[^>]*)>(.*?)</linearGradient>', re.S)
STOP = re.compile(r'<stop\b[^>]*offset="([^"]+)"[^>]*stop-color="([^"]+)"')


def gradient_defs(text):
    """Alle linearGradient-Definitionen als {id: (x1, y1, x2, y2, stops)}.

    Die Koordinaten stehen in `objectBoundingBox`, also als Anteile der
    Fläche, die den Verlauf trägt. Android will Nutzerkoordinaten; die
    Umrechnung braucht deshalb die Tintenbox der Zeichnung.
    """
    found = {}
    for head, name, body in GRADIENT.findall(text):
        units = re.search(r'gradientUnits="([^"]+)"', head)
        if units and units.group(1) != "objectBoundingBox":
            raise SystemExit(f"gradientUnits={units.group(1)} wird nicht abgebildet")
        stops = [(float(o), c) for o, c in STOP.findall(body)]
        if len(stops) < 2:
            raise SystemExit(f"Verlauf {name} mit {len(stops)} Farbmarke(n)")
        box = [float(re.search(rf'\b{axis}="([^"]+)"', head).group(1))
               if re.search(rf'\b{axis}="([^"]+)"', head) else default
               for axis, default in (("x1", 0.0), ("y1", 0.0),
                                     ("x2", 1.0), ("y2", 0.0))]
        found[name] = (*box, stops)
    return found


def check_flat(fill, attrs, gradients):
    """VectorDrawable kennt `url(#…)` nur als eingebettete `aapt:attr`.

    Ein Verweis auf eine Verlaufsdefinition landete früher wörtlich in
    `android:fillColor`; Android rendert das als Schwarz — die Zeichnung
    kompiliert, sieht aber anders aus als die Quelle. Bekannte Verläufe
    werden jetzt übersetzt, unbekannte brechen ab.
    """
    if not fill.startswith("url("):
        return fill
    name = fill[fill.index("#") + 1:fill.index(")")]
    if name not in gradients:
        raise SystemExit(
            f"Verlauf {name} ist nicht definiert: {attrs.strip()[:60]}")
    return fill


def ink_bounds_108(svg_path, transform):
    """Die Tintenbox der Zeichnung, umgerechnet aufs 108er Raster."""
    _, _, vw, vh = view_box(Path(svg_path).read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory() as tmp:
        png = Path(tmp) / "probe.png"
        height = max(1, round(PROBE_WIDTH * vh / vw))
        subprocess.run(
            ["rsvg-convert", "-w", str(PROBE_WIDTH), "-h", str(height),
             str(svg_path), "-o", str(png)],
            check=True, capture_output=True)
        fx0, fy0, fx1, fy1 = ink_box(png)
    return (transform.x(fx0 * vw), transform.y(fy0 * vh),
            transform.x(fx1 * vw), transform.y(fy1 * vh))


def gradient_block(spec, bounds, indent="        "):
    """Ein `aapt:attr`-Verlauf in Nutzerkoordinaten des 108er Rasters.

    Die SVG-Koordinaten sind Anteile der tragenden Fläche. Für ein Zeichen,
    dessen Teilflächen sich **einen** Verlauf teilen, ist das die Tintenbox
    der ganzen Zeichnung — nicht die jeder einzelnen Form. Pro Form
    gerechnet bekäme jeder Balken seinen eigenen kleinen Verlauf, und aus
    einem durchlaufenden Farbverlauf würden vier gestreifte Klötze.
    """
    x1, y1, x2, y2, stops = spec
    bx0, by0, bx1, by1 = bounds
    w, h = bx1 - bx0, by1 - by0
    items = "".join(
        f'{indent}        <item android:offset="{offset:g}" '
        f'android:color="{colour}"/>\n'
        for offset, colour in stops)
    return (f'{indent}<aapt:attr name="android:fillColor">\n'
            f'{indent}    <gradient android:type="linear"\n'
            f'{indent}        android:startX="{bx0 + x1 * w:.3f}" '
            f'android:startY="{by0 + y1 * h:.3f}"\n'
            f'{indent}        android:endX="{bx0 + x2 * w:.3f}" '
            f'android:endY="{by0 + y2 * h:.3f}">\n'
            f"{items}"
            f"{indent}    </gradient>\n"
            f"{indent}</aapt:attr>\n")


def convert(src, mono, fixed_offset=None, colour_map=None,
            mono_fill="#000000", tint=None):
    text = Path(src).read_text(encoding="utf-8")
    transform = (fixed_transform(src, fixed_offset)
                 if fixed_offset is not None else build_transform(src))
    colour_map = colour_map or {}
    gradients = gradient_defs(text)
    bounds = ink_bounds_108(src, transform) if gradients else None
    inherited = GROUP_FILL.search(text)
    inherited = inherited.group(1) if inherited else None
    shapes = []
    for match in SHAPE.finditer(text):
        kind, attrs = match.group(1), match.group(2)
        fill = mono_fill if mono else check_flat(resolve_fill(attrs, inherited),
                                                 attrs, gradients)
        if fill == "none":
            continue
        if fill == "currentColor":
            fill = mono_fill
        fill = colour_map.get(fill.upper(), fill)
        fill_type = "evenOdd" if "evenodd" in attrs else "nonZero"
        if kind == "path":
            d = re.search(r'\sd="([^"]+)"', attrs, re.S).group(1)
            shapes.append((scale_path(d, transform), fill, fill_type))
        elif kind in ("circle", "ellipse"):
            def number(key):
                found = re.search(rf'{key}="([^"]+)"', attrs)
                if not found:
                    raise SystemExit(f"{kind} ohne {key}")
                return float(found.group(1))
            rx = number("r") if kind == "circle" else number("rx")
            ry = rx if kind == "circle" else number("ry")
            shapes.append((ellipse_path(number("cx"), number("cy"), rx, ry,
                                        transform), fill, fill_type))
        else:
            def number(key, default=None):
                found = re.search(rf'{key}="([^"]+)"', attrs)
                if found:
                    return float(found.group(1))
                if default is not None:
                    return default
                raise SystemExit(f"{kind} without {key}")
            rx = number("rx", 0.0)
            ry = number("ry", rx)
            shapes.append((
                rounded_rect_path(number("x"), number("y"), number("width"),
                                  number("height"), rx, ry, transform),
                fill,
                fill_type,
            ))
    if not shapes:
        raise SystemExit(f"keine Formen in {src}")

    uses_gradient = any(f.startswith("url(") for _, f, _ in shapes)
    head = ('<?xml version="1.0" encoding="utf-8"?>\n'
            "<!-- Generated by scripts/build-brand-assets.sh."
            " Do not edit by hand. -->\n"
            '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
            + ('    xmlns:aapt="http://schemas.android.com/aapt"\n'
               if uses_gradient else "")
            + (f'    android:tint="{tint}"\n' if tint else "")
            + '    android:width="108dp"\n'
            '    android:height="108dp"\n'
            '    android:viewportWidth="108"\n'
            '    android:viewportHeight="108">\n')
    parts = []
    for d, fill, fill_type in shapes:
        if fill.startswith("url("):
            name = fill[fill.index("#") + 1:fill.index(")")]
            parts.append(
                f'    <path android:fillType="{fill_type}"\n'
                f'          android:pathData="{d}">\n'
                + gradient_block(gradients[name], bounds)
                + "    </path>\n")
        else:
            parts.append(
                f'    <path android:fillColor="{fill}" '
                f'android:fillType="{fill_type}"\n'
                f'          android:pathData="{d}"/>\n')
    return head + "".join(parts) + "</vector>\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source")
    parser.add_argument("destination")
    parser.add_argument("--mono", action="store_true",
                        help="replace every shape fill with --mono-fill")
    parser.add_argument("--mono-fill", default="#000000")
    parser.add_argument("--fixed-offset", type=float,
                        help="translate by this amount without scaling")
    parser.add_argument("--colour-map", action="append", default=[],
                        metavar="SVG=ANDROID",
                        help="map a literal SVG colour to an Android resource")
    parser.add_argument("--tint", help="optional android:tint on the vector")
    args = parser.parse_args()
    colour_map = {}
    for item in args.colour_map:
        if "=" not in item:
            parser.error(f"--colour-map needs SVG=ANDROID, got {item!r}")
        source, target = item.split("=", 1)
        colour_map[source.upper()] = target
    Path(args.destination).write_text(
        convert(args.source, args.mono, args.fixed_offset, colour_map,
                args.mono_fill, args.tint), encoding="utf-8")


if __name__ == "__main__":
    main()
