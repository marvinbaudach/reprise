#!/usr/bin/env python3
"""Bildet die Palette einer Zeichnung auf eine andere ab.

Die Fassung für dunkle Gründe ist keine zweite Zeichnung, sondern dieselbe
mit angehobenen Körperwerten. Als Kopie gepflegt läuft sie auseinander;
erzeugt kann sie es nicht.

Alle Ersetzungen laufen in **einem** Durchgang. Nacheinander angewandt
würde `#1F1056 → #3A2470` von einer späteren Regel `#3A2470 → …` noch
einmal angefasst, und die Palette rutscht still durch.
"""
import argparse
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from svg_ids import prefix_ids                          # noqa: E402

# Auch `currentColor` ist ein Farbwert und muss abbildbar sein: die
# einfarbige Fassung benutzt ihn, damit eine Webseite sie per CSS einfärben
# kann — das GNOME-Symbolic braucht dort aber einen literalen Wert.
_HEX = re.compile(r"#[0-9A-Fa-f]{6}\b|\bcurrentColor\b")


def recolour(text, mapping):
    upper = {k.upper(): v for k, v in mapping.items()}
    hit = set()

    def swap(match):
        key = match.group(0).upper()
        if key in upper:
            hit.add(key)
            return upper[key]
        return match.group(0)

    out = _HEX.sub(swap, text)
    missing = sorted(set(upper) - hit)
    if missing:
        # Eine Regel ohne Treffer heißt: die Quelle hat sich geändert und die
        # Abbildung nicht. Still durchwinken hieße, die Fassung für dunkle
        # Gründe schweigend zu entwerten.
        raise SystemExit("Palette ohne Treffer: " + ", ".join(missing))
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source")
    parser.add_argument("destination")
    parser.add_argument("mapping", nargs="+", metavar="ALT=NEU")
    parser.add_argument("--prefix", default="",
                        help="Präfix für alle id-Werte dieser Datei")
    args = parser.parse_args()
    mapping = dict(pair.split("=", 1) for pair in args.mapping)
    text = Path(args.source).read_text(encoding="utf-8")
    out = recolour(text, mapping)
    if args.prefix:
        out = prefix_ids(out, args.prefix)
    header = ("<!-- Erzeugt von scripts/build-brand-assets.sh. "
              "Nicht von Hand ändern. -->\n")
    Path(args.destination).write_text(out.replace("<svg", header + "<svg", 1),
                                      encoding="utf-8")


if __name__ == "__main__":
    main()
