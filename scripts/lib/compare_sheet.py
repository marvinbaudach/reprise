#!/usr/bin/env python3
"""Build the self-contained dock-size comparison sheet for both variants."""

import argparse
import base64
import html
import subprocess
import tempfile
from pathlib import Path

SIZES = (128, 48, 28, 24, 16)


def png_data(source, size):
    with tempfile.TemporaryDirectory() as tmp:
        png = Path(tmp) / "render.png"
        subprocess.run(
            ["rsvg-convert", "-w", str(size), "-h", str(size),
             str(source), "-o", str(png)],
            check=True,
            capture_output=True,
        )
        encoded = base64.b64encode(png.read_bytes()).decode("ascii")
    return f"data:image/png;base64,{encoded}"


def image(source, size, label, preview=False):
    cls = ' class="preview"' if preview else ""
    shown = size * 4 if preview else size
    return (f'<figure><img{cls} src="{png_data(source, size)}" width="{shown}" '
            f'height="{shown}" alt="{html.escape(label)}"><figcaption>'
            f'{html.escape(label)}</figcaption></figure>')


def row(variant, size, ground, transparent, plate):
    items = [image(plate, size, "on plate"), image(transparent, size, "transparent")]
    if size <= 28:
        items.extend((image(plate, size, "plate 4×", True),
                      image(transparent, size, "transparent 4×", True)))
    return (f'<div class="row {ground}"><h3>{size} px</h3>'
            + "".join(items) + "</div>")


def section(name, normal, light, plate):
    rows = []
    for size in SIZES:
        rows.append(row(name, size, "dark", normal, plate))
        rows.append(row(name, size, "light", light, plate))
    return f'<section><h2>Variant {name.upper()}</h2>{"".join(rows)}</section>'


def build(destination, sources):
    body = "".join(section(*item) for item in sources)
    document = f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Reprise repeat-sign variants</title>
<style>
*{{box-sizing:border-box}} body{{margin:0;background:#171820;color:#f4f4f7;font:14px system-ui,sans-serif}}
main{{max-width:1100px;margin:auto;padding:24px}} section{{margin:28px 0}} h1,h2,h3{{margin:0}}
.row{{display:flex;align-items:center;gap:18px;min-height:168px;padding:16px;margin:8px 0;border-radius:14px}}
.row.dark{{background:#0a0a0e}} .row.light{{background:#eceef5;color:#171820}}
.row h3{{width:58px}} figure{{margin:0;text-align:center;min-width:92px}} img{{display:block;margin:auto}}
.preview{{image-rendering:pixelated}} figcaption{{margin-top:6px;font-size:12px;opacity:.75}}
</style></head><body><main><h1>Reprise repeat-sign variants</h1>
<p>Every image is embedded. Light rows use the light-ground mark; plate samples keep the dock colours.</p>
{body}</main></body></html>
'''
    Path(destination).parent.mkdir(parents=True, exist_ok=True)
    Path(destination).write_text(document, encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination")
    parser.add_argument("--a", nargs=3, metavar=("NORMAL", "LIGHT", "PLATE"), required=True)
    parser.add_argument("--b", nargs=3, metavar=("NORMAL", "LIGHT", "PLATE"), required=True)
    args = parser.parse_args()
    build(args.destination, (("a", *map(Path, args.a)), ("b", *map(Path, args.b))))


if __name__ == "__main__":
    main()
