#!/usr/bin/env python3
"""Pixel-, Pfad- und Kontrastmessungen für das Logo-Gate.

Getrennt vom Shell-Skript, weil Zusammenhangskomponenten und
Kontrastarithmetik in awk nur schwer nachvollziehbar wären.

Alle Kontraste werden **flächengewichtet am gerenderten Bild** gemessen,
nicht an einzelnen Hex-Werten aus der Datei. Ein einzelner Glanzpunkt darf
sonst über eine Marke entscheiden, die auf ihrer ganzen Fläche im Grund
versinkt — genau dieser Fehler steckte in der ersten Fassung.
"""
import math
import re
import sys
from collections import deque

from PIL import Image

# Alle Formelemente, die Fläche tragen. <path> allein zu zählen unterschätzt
# eine Zeichnung, die ihre Ringe als <ellipse> baut.
SHAPE_TAGS = ("path", "ellipse", "circle", "rect", "polygon", "polyline", "line")
PATH_CMD = re.compile(r"[MmLlHhVvCcSsQqTtAaZz]")

# Ab wann gilt eine Hintergrundinsel als echter Negativraum: drei Pixel oder
# ein Promille der Fläche, je nachdem was größer ist. Zwei Pixel Restluft
# zwischen zwei Formen sind kein Auge.
MIN_HOLE_SHARE = 0.001
MIN_HOLE_PIXELS = 3


def _rgba(png):
    return Image.open(png).convert("RGBA")


def _alpha_mask(png):
    """True = Hintergrund (transparent), False = Marke."""
    img = _rgba(png)
    w, h = img.size
    a = img.getchannel("A").load()
    return w, h, [[a[x, y] < 128 for x in range(w)] for y in range(h)]


def bg_components(png):
    """Zahl der Hintergrund-Zusammenhangskomponenten, 4er-Nachbarschaft.

    Ein Klumpen ohne Aussparung hat genau 1: den Außenraum. Jede weitere
    Komponente ist überlebender Negativraum — sofern sie groß genug ist, um
    bei dieser Rendergröße noch als Aussparung gesehen zu werden.
    """
    w, h, bg = _alpha_mask(png)
    floor = max(MIN_HOLE_PIXELS, int(MIN_HOLE_SHARE * w * h))
    seen = [[False] * w for _ in range(h)]
    count = 0
    for sy in range(h):
        for sx in range(w):
            if seen[sy][sx] or not bg[sy][sx]:
                continue
            q = deque([(sx, sy)])
            seen[sy][sx] = True
            area = 0
            touches_border = False
            while q:
                x, y = q.popleft()
                area += 1
                if x in (0, w - 1) or y in (0, h - 1):
                    touches_border = True
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < w and 0 <= ny < h and not seen[ny][nx] and bg[ny][nx]:
                        seen[ny][nx] = True
                        q.append((nx, ny))
            # Der Außenraum zählt immer, Inseln erst ab der Mindestgröße.
            if touches_border or area >= floor:
                count += 1
    return count


def fill_ratio(png):
    """Anteil der Live-Fläche, den die Bounding-Box der Marke belegt."""
    w, h, bg = _alpha_mask(png)
    xs = [x for y in range(h) for x in range(w) if not bg[y][x]]
    ys = [y for y in range(h) for x in range(w) if not bg[y][x]]
    if not xs:
        return 0.0, 0.0
    return (max(xs) - min(xs) + 1) / w, (max(ys) - min(ys) + 1) / h


def ink_box(png):
    """Bounding-Box der Marke in Anteilen der Bildfläche: x0 y0 x1 y1.

    Optische Größe richtet sich nach der gezeichneten Fläche, nicht nach dem
    viewBox — sonst sitzt jede Stufe anders auf der Platte.
    """
    w, h, bg = _alpha_mask(png)
    xs = [x for y in range(h) for x in range(w) if not bg[y][x]]
    ys = [y for y in range(h) for x in range(w) if not bg[y][x]]
    if not xs:
        raise SystemExit("leeres Bild")
    return min(xs) / w, min(ys) / h, (max(xs) + 1) / w, (max(ys) + 1) / h


def shape_stats(svg):
    """Zahl der flächentragenden Formen und die größte Befehlszahl eines Pfades."""
    text = open(svg, encoding="utf-8").read()
    shapes = sum(len(re.findall(rf"<{tag}\b", text)) for tag in SHAPE_TAGS)
    ds = re.findall(r'\sd\s*=\s*"([^"]*)"', text)
    longest = max((len(PATH_CMD.findall(d)) for d in ds), default=0)
    return shapes, longest


def _luminance_rgb(r, g, b):
    parts = [c / 255 for c in (r, g, b)]
    lin = [c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4 for c in parts]
    return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]


def _luminance(hex_colour):
    hex_colour = hex_colour.lstrip("#")
    if len(hex_colour) == 3:
        hex_colour = "".join(c * 2 for c in hex_colour)
    return _luminance_rgb(*(int(hex_colour[i:i + 2], 16) for i in (0, 2, 4)))


def contrast(fg, bg):
    a, b = _luminance(fg), _luminance(bg)
    hi, lo = max(a, b), min(a, b)
    return (hi + 0.05) / (lo + 0.05)


def _opaque_pixels(png):
    """Alle voll deckenden Pixel der Marke. Kantenpixel bleiben draußen —
    ihre Mischfarbe stammt vom Renderer, nicht von der Zeichnung."""
    img = _rgba(png)
    w, h = img.size
    px = img.load()
    out = []
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a >= 250:
                out.append((x, y, r, g, b))
    return w, h, out


def _contrast_series(pixels, ground_hex):
    ground = _luminance(ground_hex)
    cache = {}
    values = []
    for _, _, r, g, b in pixels:
        key = (r, g, b)
        if key not in cache:
            lum = _luminance_rgb(r, g, b)
            hi, lo = max(lum, ground), min(lum, ground)
            cache[key] = (hi + 0.05) / (lo + 0.05)
        values.append(cache[key])
    values.sort()
    return values


def _report(values):
    if not values:
        return "0.00 0.00 0.0000 1.0000"
    median = values[len(values) // 2]
    share_3 = sum(1 for v in values if v >= 3.0) / len(values)
    share_blind = sum(1 for v in values if v < 1.5) / len(values)
    return f"{median:.2f} {values[0]:.2f} {share_3:.4f} {share_blind:.4f}"


def ground_contrast(png, ground_hex):
    """Median, Minimum, Anteil ≥ 3:1 und Anteil < 1,5:1 über die ganze Marke."""
    _, _, pixels = _opaque_pixels(png)
    return _report(_contrast_series(pixels, ground_hex))


def edge_contrast(png, ground_hex, depth=2):
    """Dasselbe, aber nur für den Saum der Marke.

    Ob eine Marke auf einem Grund steht, entscheidet ihre Außenkante. Das
    Innere darf beliebig hell sein, solange der Rand trägt.
    """
    w, h, bg = _alpha_mask(png)
    img = _rgba(png)
    px = img.load()
    rim = []
    for y in range(h):
        for x in range(w):
            if bg[y][x] or px[x, y][3] < 250:
                continue
            near_bg = any(
                bg[ny][nx]
                for dy in range(-depth, depth + 1)
                for dx in range(-depth, depth + 1)
                for nx, ny in ((x + dx, y + dy),)
                if 0 <= nx < w and 0 <= ny < h
            )
            if near_bg:
                r, g, b, _ = px[x, y]
                rim.append((x, y, r, g, b))
    return _report(_contrast_series(rim, ground_hex))


def pair_contrast(mark_png, ground_png):
    """Kontrast Marke gegen einen **gerenderten** Grund, Pixel für Pixel.

    Ein Verlauf hat keinen einzelnen Hex-Wert. Für die Platte des App-Icons
    muss deshalb jedes Markenpixel gegen genau das Pixel gemessen werden,
    das unter ihm liegt.
    """
    mark, ground = _rgba(mark_png), _rgba(ground_png)
    if mark.size != ground.size:
        raise SystemExit(f"Maße verschieden: {mark.size} vs {ground.size}")
    mp, gp = mark.load(), ground.load()
    w, h = mark.size
    cache = {}
    values = []
    for y in range(h):
        for x in range(w):
            r, g, b, a = mp[x, y]
            if a < 250:
                continue
            gr, gg, gb, _ = gp[x, y]
            key = (r, g, b, gr, gg, gb)
            if key not in cache:
                lm, lg = _luminance_rgb(r, g, b), _luminance_rgb(gr, gg, gb)
                hi, lo = max(lm, lg), min(lm, lg)
                cache[key] = (hi + 0.05) / (lo + 0.05)
            values.append(cache[key])
    values.sort()
    return _report(values)


def pair_edge_contrast(mark_png, ground_png, depth=2):
    """Wie `pair_contrast`, aber nur für den Saum der Marke.

    Über die ganze Fläche gemessen fällt eine dunkle Marke auf einer
    dunklen Stelle ihrer eigenen Platte durch — obwohl sie dort gar nicht
    gegen die Platte antritt, sondern sie verdeckt. Was entscheidet, ob die
    Marke auf der Platte steht, ist die Außenkante.
    """
    mark, ground = _rgba(mark_png), _rgba(ground_png)
    if mark.size != ground.size:
        raise SystemExit(f"Maße verschieden: {mark.size} vs {ground.size}")
    w, h, bg = _alpha_mask(mark_png)
    mp, gp = mark.load(), ground.load()
    cache, values = {}, []
    for y in range(h):
        for x in range(w):
            if bg[y][x] or mp[x, y][3] < 250:
                continue
            near_bg = any(
                bg[ny][nx]
                for dy in range(-depth, depth + 1)
                for dx in range(-depth, depth + 1)
                for nx, ny in ((x + dx, y + dy),)
                if 0 <= nx < w and 0 <= ny < h
            )
            if not near_bg:
                continue
            r, g, b, _ = mp[x, y]
            gr, gg, gb, _ = gp[x, y]
            key = (r, g, b, gr, gg, gb)
            if key not in cache:
                lm, lg = _luminance_rgb(r, g, b), _luminance_rgb(gr, gg, gb)
                hi, lo = max(lm, lg), min(lm, lg)
                cache[key] = (hi + 0.05) / (lo + 0.05)
            values.append(cache[key])
    values.sort()
    return _report(values)


def colour_components(png, hex_colour, tolerance=60, min_share=0.001):
    """Zahl der zusammenhängenden Flächen in der Nähe einer Farbe.

    Der Negativraum-Test greift nur bei Silhouetten: eine farbige Zeichnung
    hat keine durchsichtigen Löcher, ihre Augen sind gefüllt. Was dort
    zählt, ist, ob die beiden Augen bei der Zielgröße noch zwei getrennte
    Flächen sind — oder ob sie mit dem Kopf verschmolzen sind.
    """
    target = hex_colour.lstrip("#")
    tr, tg, tb = (int(target[i:i + 2], 16) for i in (0, 2, 4))
    img = _rgba(png)
    w, h = img.size
    px = img.load()
    hit = [[False] * w for _ in range(h)]
    for y in range(h):
        for x in range(w):
            r, g, b, a = px[x, y]
            if a < 128:
                continue
            if abs(r - tr) + abs(g - tg) + abs(b - tb) <= tolerance:
                hit[y][x] = True
    # Kantenpixel treffen die Zielfarbe zufällig. Ohne Mindestgröße zählt
    # der Test antialiastes Rauschen als Auge und meldet Hunderte Flächen.
    floor = max(2, int(min_share * w * h))
    seen = [[False] * w for _ in range(h)]
    count = 0
    for sy in range(h):
        for sx in range(w):
            if seen[sy][sx] or not hit[sy][sx]:
                continue
            queue = deque([(sx, sy)])
            seen[sy][sx] = True
            area = 0
            while queue:
                x, y = queue.popleft()
                area += 1
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < w and 0 <= ny < h and not seen[ny][nx] and hit[ny][nx]:
                        seen[ny][nx] = True
                        queue.append((nx, ny))
            if area >= floor:
                count += 1
    return count


def radius(png, cx_share=0.5, cy_share=0.5):
    """Größter Abstand eines deckenden Pixels vom Mittelpunkt, in Bildanteilen.

    Für Androids garantierte Kreisfläche: der Rückgabewert mal Viewport-Größe
    ergibt den Radius in dp.
    """
    w, h, bg = _alpha_mask(png)
    cx, cy = cx_share * w, cy_share * h
    best = 0.0
    for y in range(h):
        for x in range(w):
            if bg[y][x]:
                continue
            best = max(best, math.hypot(x + 0.5 - cx, y + 0.5 - cy))
    return best / w


def overlap(png_a, png_b):
    """Jaccard-Index zweier Alphamasken.

    Damit lässt sich beweisen, dass die Pfadfassung eines Schriftzugs
    tatsächlich dasselbe zeigt wie die Live-Text-Fassung.
    """
    wa, ha, bga = _alpha_mask(png_a)
    wb, hb, bgb = _alpha_mask(png_b)
    if (wa, ha) != (wb, hb):
        raise SystemExit(f"Maße verschieden: {wa}x{ha} vs {wb}x{hb}")
    inter = union = 0
    for y in range(ha):
        for x in range(wa):
            a, b = not bga[y][x], not bgb[y][x]
            inter += a and b
            union += a or b
    return inter / union if union else 0.0


def main():
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == "bg-components":
        print(bg_components(args[0]))
    elif cmd == "fill-ratio":
        fw, fh = fill_ratio(args[0])
        print(f"{fw:.4f} {fh:.4f}")
    elif cmd == "ink-box":
        print(" ".join(f"{v:.6f}" for v in ink_box(args[0])))
    elif cmd == "shape-stats":
        n, m = shape_stats(args[0])
        print(f"{n} {m}")
    elif cmd == "contrast":
        print(f"{contrast(args[0], args[1]):.2f}")
    elif cmd == "ground-contrast":
        print(ground_contrast(args[0], args[1]))
    elif cmd == "edge-contrast":
        print(edge_contrast(args[0], args[1]))
    elif cmd == "pair-contrast":
        print(pair_contrast(args[0], args[1]))
    elif cmd == "pair-edge-contrast":
        print(pair_edge_contrast(args[0], args[1]))
    elif cmd == "colour-components":
        print(colour_components(args[0], args[1],
                                int(args[2]) if len(args) > 2 else 60))
    elif cmd == "radius":
        print(f"{radius(args[0]):.4f}")
    elif cmd == "overlap":
        print(f"{overlap(args[0], args[1]):.4f}")
    else:
        raise SystemExit(f"unknown command: {cmd}")


if __name__ == "__main__":
    main()
