#!/usr/bin/env python3
"""Setzt ein App-Icon aus Platte und Zeichnung zusammen.

Das App-Icon war vorher eine Kopie der Marke mit einer Platte davor. Zwei
Kopien derselben Zeichnung laufen auseinander, und genau das ist passiert:
die kleinen Stufen bekamen die Platte nie. Hier wird das Icon erzeugt, also
kann es nicht mehr abweichen.

Die Marke wird an ihrer **gezeichneten Fläche** ausgerichtet, nicht am
viewBox. Sonst sitzt jede Stufe anders auf der Platte, weil die drei
Zeichnungen ihr Raster unterschiedlich ausfüllen.
"""
import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from logo_measure import ink_box                       # noqa: E402
from svg_ids import inner, prefix_ids, view_box        # noqa: E402

PROBE_WIDTH = 512

_RECT = re.compile(r"<rect\b[^>]*/>")


def ink_bounds(svg_path, vb):
    """Bounding-Box der gezeichneten Fläche in viewBox-Einheiten."""
    _, _, vw, vh = vb
    with tempfile.TemporaryDirectory() as tmp:
        png = Path(tmp) / "probe.png"
        height = max(1, round(PROBE_WIDTH * vh / vw))
        subprocess.run(
            ["rsvg-convert", "-w", str(PROBE_WIDTH), "-h", str(height),
             str(svg_path), "-o", str(png)],
            check=True, capture_output=True)
        fx0, fy0, fx1, fy1 = ink_box(png)
    return fx0 * vw, fy0 * vh, fx1 * vw, fy1 * vh


def reframe(plate_text, inset, corner):
    """Setzt den Rahmen der Platte neu.

    Der Verlauf gehört der Marke, der Rahmen der Zielfläche: GNOME will eine
    abgerundete, eingerückte Platte, Apple und der Play Store wollen sie
    randlos, weil sie selbst maskieren. Beides aus derselben Datei zu
    erzeugen ist der einzige Weg, den Verlauf nicht zweimal zu pflegen.
    """
    rects = _RECT.findall(plate_text)
    if len(rects) != 1:
        raise SystemExit(f"Platte braucht genau ein <rect>, gefunden: {len(rects)}")
    _, _, width, height = view_box(plate_text)
    rest = re.sub(r'\s*\b(x|y|width|height|rx|ry)="[^"]*"', "", rects[0])
    frame = (f'x="{inset:g}" y="{inset:g}" '
             f'width="{width - 2 * inset:g}" height="{height - 2 * inset:g}" '
             f'rx="{corner:g}"')
    return plate_text.replace(rects[0], rest.replace("<rect", f"<rect {frame}", 1))


def compose(plate_path, mark_path, box_w, box_h, cx, cy, inset=None, corner=None):
    plate_text = Path(plate_path).read_text(encoding="utf-8")
    if inset is not None:
        plate_text = reframe(plate_text, inset, corner)
    mark_text = Path(mark_path).read_text(encoding="utf-8")
    plate_vb = view_box(plate_text)
    mark_vb = view_box(mark_text)

    x0, y0, x1, y1 = ink_bounds(mark_path, mark_vb)
    scale = min(box_w / (x1 - x0), box_h / (y1 - y0))
    tx = cx - scale * (x0 + x1) / 2
    ty = cy - scale * (y0 + y1) / 2

    mark_inner = prefix_ids(inner(mark_text), "rp-m-")
    width, height = plate_vb[2], plate_vb[3]
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" '
        f'viewBox="0 0 {width:g} {height:g}" '
        f'width="{width:g}" height="{height:g}">\n'
        "  <!-- Erzeugt von scripts/build-brand-assets.sh. Nicht von Hand ändern. -->\n"
        f"{inner(plate_text)}\n"
        f'  <g id="rp-mark" transform="translate({tx:.4f} {ty:.4f}) '
        f'scale({scale:.6f})">\n{mark_inner}\n  </g>\n'
        "</svg>\n"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("plate")
    parser.add_argument("mark")
    parser.add_argument("destination")
    parser.add_argument("--box-width", type=float, required=True)
    parser.add_argument("--box-height", type=float, required=True)
    parser.add_argument("--cx", type=float, default=64.0)
    parser.add_argument("--cy", type=float, default=64.0)
    parser.add_argument("--plate-inset", type=float,
                        help="Rahmen der Platte neu setzen: Einrückung")
    parser.add_argument("--plate-radius", type=float, default=0.0,
                        help="Eckenradius zum neu gesetzten Rahmen")
    args = parser.parse_args()
    Path(args.destination).write_text(
        compose(args.plate, args.mark, args.box_width, args.box_height,
                args.cx, args.cy, args.plate_inset, args.plate_radius),
        encoding="utf-8")


if __name__ == "__main__":
    main()
