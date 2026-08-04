#!/usr/bin/env python3
"""Löst eine Ebene eines zusammengesetzten SVG als eigene Datei heraus.

Das Gate muss die Marke gegen ihre Platte messen. Auf dem fertigen Icon
liegt die Marke über der Platte, dort ist das darunterliegende Pixel nicht
mehr auslesbar. Beide Ebenen einzeln gerendert lassen sich Pixel für Pixel
gegeneinander halten — das ist die einzige ehrliche Kontrastmessung für
einen Verlauf, der keinen einzelnen Hex-Wert hat.

`<defs>` wandern immer mit: die Verlaufsdefinition der Platte steht im
Wurzelknoten, der Verweis darauf in der Ebene.
"""
import argparse
import re
from pathlib import Path

_DEFS = re.compile(r"<defs\b.*?</defs>", re.S)


def _group(text, group_id):
    """Der vollständige `<g id="…">…</g>`-Block, Verschachtelung mitgezählt."""
    start = re.search(rf'<g\b[^>]*\bid="{re.escape(group_id)}"[^>]*>', text)
    if not start:
        raise SystemExit(f"Ebene {group_id} nicht gefunden")
    depth, position = 0, start.start()
    for match in re.finditer(r"<g\b[^>]*?(/?)>|</g>", text[start.start():]):
        if match.group(0) == "</g>":
            depth -= 1
        elif not match.group(1):
            depth += 1
        if depth == 0:
            return text[position:start.start() + match.end()]
    raise SystemExit(f"Ebene {group_id} ist nicht geschlossen")


def extract(text, group_id):
    head = re.search(r"<svg\b[^>]*>", text).group(0)
    defs = "\n".join(_DEFS.findall(text))
    return f"{head}\n{defs}\n{_group(text, group_id)}\n</svg>\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source")
    parser.add_argument("group_id")
    parser.add_argument("destination")
    args = parser.parse_args()
    Path(args.destination).write_text(
        extract(Path(args.source).read_text(encoding="utf-8"), args.group_id),
        encoding="utf-8")


if __name__ == "__main__":
    main()
