---
slug: owl-logo
worktree: /home/marvin/Projects/reprise/.worktrees/owl-logo
branch: feat/owl-logo
phase: verified
codex_session:
created: 2026-08-04
---
# Owl Logo Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Das Reprise-Logo als handgezeichneten Vektor in drei größenabhängigen
Zeichnungen neu bauen und daraus GNOME, Android und Web bedienen.

**Architecture:** Drei SVG-Master (`full`, `reduced`, `micro`) unter
`data/brand/` sind die einzige Quelle.
> **Überholt.** Die Marke ist keine Eule mehr, sondern das
> Wiederholungszeichen der Notenschrift. Es gibt `data/brand/mark.svg` und
> die einfarbige `mark-mono.svg`, beide auf dem 16er Raster. Begründung in
> der Spec unter „Richtungswechsel: vom Gesicht zum Zeichen". Alle Zielflächen sind reine Ableitungen
davon. Ein Mess-Skript prüft jede Zeichnung gegen die Kriterien der Spec, bevor
irgendetwas abgeleitet wird — die Zeichnung wird gemessen, nicht begutachtet.

**Tech Stack:** SVG 1.1, `rsvg-convert` 2.62, ImageMagick 7.1, Python 3.14 +
Pillow, `fonttools` 4.63, Inkscape 1.4.4, Android VectorDrawable, meson.

Alle genannten Werkzeuge sind auf diesem Rechner vorhanden und wurden vor dem
Schreiben dieses Plans geprüft. Inkscape wird für die Pfadvereinigung im
Symbolic gebraucht; die Text→Pfad-Wandlung läuft über `fonttools`, weil die
gepinnte statische Fraunces-Instanz dort ohnehin erzeugt wird.

**Spec:** `docs/superpowers/specs/2026-08-03-owl-logo-monochrome-design.md`

## Global Constraints

- Alle Zeichnungen sind **handgezeichnet**. Es wird nichts getract. Die
  Signatur des Tracens — ein Pfad mit Tausenden Befehlen — ist ein
  Abbruchkriterium (V5).
- Zielraster `full`: `viewBox="0 0 1247 1000"`. Das Seitenverhältnis 1,2467
  stammt aus der Vermessung der bereinigten Vorlage.
- Palette exakt: Tinte `#1F1056`, Kopf dunkel `#2B155E`, Kopf mittel `#452674`,
  Körper hell `#5F2F8A`, Gesicht hell `#8A679C`, Bügel-Teal `#4F93C8`,
  Bügel-Aufhellung `#8292D5`, Muschel-Akzent `#5798BD`, Auge Teal `#1C698A`,
  Auge tief `#114A71`, Metall `#A09EB4`→`#E0E0E1`. Grundfläche des
  GNOME-App-Icons `#1B082D`.
  > **Überholt.** Die Grundfläche ist eine Verlaufsplatte `#5798BD → #8570CB`,
  > der Kopf-Verlauf endet bei `#33195F`, und `Metall` entfällt. Gründe und
  > Messwerte stehen in der Spec unter „Änderungen aus der Umsetzung".
- GNOME-Symbolic: genau **ein** `<path>`, `viewBox="0 0 16 16"`, kein
  `transform`, kein `stroke`, kein Verlauf, `fill="#222222"`.
- Android: `minSdk = 26`. Adaptive Icons sind garantiert, Legacy-PNG-Buckets
  entfallen. Alle Layer auf 108-dp-Viewport, Marke vollständig in der
  72-dp-Safe-Zone.
  > **Überholt.** Androids auf *jeder* Maskenform garantierter Bereich ist der
  > **66-dp-Kreis**, nicht das 72-dp-Quadrat. Die Marke wird auf diesen Radius
  > gepasst und gemessen (V9).
- Referenz für Vorlagentreue ist ausschließlich
  `docs/assets/brand-reference/owl-headphones-template-clean.png`, nie die
  Fassung mit eingebranntem Karomuster.
- Commit-Messages englisch, Dokumentation und Kommentare deutsch — wie im
  restlichen Repo.
- Gearbeitet wird im Worktree `.worktrees/owl-logo` auf `feat/owl-logo`.

## Gemessene Landmarken

Alle Werte in `full`-Koordinaten (`viewBox="0 0 1247 1000"`), aus der
bereinigten Vorlage gemessen:

| Landmarke | Wert |
|---|---|
| Bügel auf der Mittelachse | y = 0 … 145 (Dicke 145 am Scheitel) |
| Kopfoberkante auf der Mittelachse | y = 427 |
| Kinn | y = 999, x = 402 … 843 |
| Breiteste Stelle (Ohrmuscheln) | y = 674, x = 0 … 1247 |
| Auge links, Zentrum | (446, 757), rx ≈ 49, ry ≈ 50 |
| Auge rechts, Zentrum | (799, 758), rx ≈ 49, ry ≈ 50 |
| Ohrbüschel-Spitze links | (179, 200) |
| Ohrbüschel-Spitze rechts | (1068, 200) |

## File Structure

```
scripts/check-logo-artwork.sh              NEU  Mess-Gate für alle Zeichnungen
scripts/lib/logo_measure.py                NEU  Pixel- und Pfadmessungen
scripts/lib/svg_to_vectordrawable.py       NEU  SVG → Android VectorDrawable
scripts/lib/wordmark_to_path.py            NEU  Schriftzug → SVG-Pfaddaten

data/brand/fonts/Fraunces-SemiBold.ttf     NEU  gepinnte statische Instanz

data/brand/mark.svg                        NEU  full, farbig, freistehend (= Web-Marke)
data/brand/mark-on-dark.svg                NEU  full, für dunkle Gründe
data/brand/mark-mono.svg                   NEU  full, eine Füllung, currentColor
data/brand/mark-reduced.svg                NEU  reduced
data/brand/mark-micro.svg                  NEU  micro, genau ein Pfad
data/brand/lockup-horizontal.svg           NEU
data/brand/lockup-vertical.svg             NEU
data/brand/lockup-horizontal-outlined.svg  NEU
data/brand/lockup-vertical-outlined.svg    NEU
data/brand/favicon.svg                     NEU
data/brand/favicon-32.png                  NEU
data/brand/apple-touch-icon-180.png        NEU
data/brand/play-store-icon-512.png         NEU

data/icons/hicolor/scalable/apps/org.reprise.Reprise.svg          NEU (wiederhergestellt)
data/icons/hicolor/{48,64,128,256,512}x…/apps/org.reprise.Reprise.png   NEU
data/icons/hicolor/symbolic/apps/org.reprise.Reprise-symbolic.svg ERSETZT
data/meson.build                                                  GEÄNDERT

android/app/src/main/res/drawable/ic_launcher_foreground.xml      NEU
android/app/src/main/res/drawable/ic_launcher_monochrome.xml      NEU
android/app/src/main/res/values/ic_launcher_background.xml        NEU
android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml        NEU
android/app/src/main/res/mipmap-anydpi-v26/ic_launcher_round.xml  NEU
android/app/src/main/AndroidManifest.xml                          GEÄNDERT
```

Die Trennung ist absichtlich: `scripts/lib/logo_measure.py` enthält die
Pixelarithmetik, `check-logo-artwork.sh` nur Orchestrierung und Schwellwerte.
Wer eine Schwelle ändert, muss die Messlogik nicht lesen.

---

### Task 1: Das Mess-Gate

Zuerst das Lineal, dann das Werkstück. Ohne dieses Skript ist jede
Aussage über die Zeichnungen ein Eindruck.

**Files:**
- Create: `scripts/lib/logo_measure.py`
- Create: `scripts/check-logo-artwork.sh`

**Interfaces:**
- Produces: `scripts/check-logo-artwork.sh <stage> <svg>` mit
  `stage ∈ {full, reduced, micro}`, Exit 0 bei Erfolg, 1 bei Verstoß.
  `--symbolic <svg>` prüft zusätzlich V7. `--self-test` kalibriert den
  V2-Detektor an synthetischen Fixtures.
- Produces: `python3 scripts/lib/logo_measure.py bg-components <png>` → Zahl,
  `… fill-ratio <png>` → `BREITE HÖHE` als Anteile, `… path-stats <svg>` →
  `PFADE MAXBEFEHLE`, `… contrast <hex> <hex>` → Kontrastverhältnis.

- [ ] **Step 1: Messmodul schreiben**

```python
#!/usr/bin/env python3
"""Pixel- und Pfadmessungen für das Logo-Gate.

Getrennt vom Shell-Skript, weil Zusammenhangskomponenten und
Kontrastarithmetik in awk nur schwer nachvollziehbar wären.
"""
import re
import sys
from collections import deque

from PIL import Image

PATH_CMD = re.compile(r"[MmLlHhVvCcSsQqTtAaZz]")


def _alpha_mask(png):
    """True = Hintergrund (transparent), False = Marke."""
    img = Image.open(png).convert("RGBA")
    w, h = img.size
    a = img.getchannel("A").load()
    return w, h, [[a[x, y] < 128 for x in range(w)] for y in range(h)]


def bg_components(png, min_area=2):
    """Zahl der Hintergrund-Zusammenhangskomponenten, 4er-Nachbarschaft.

    Ein Klumpen ohne Aussparung hat genau 1: den Außenraum. Jede
    zusätzliche Komponente ist überlebender Negativraum.
    """
    w, h, bg = _alpha_mask(png)
    seen = [[False] * w for _ in range(h)]
    count = 0
    for sy in range(h):
        for sx in range(w):
            if seen[sy][sx] or not bg[sy][sx]:
                continue
            q = deque([(sx, sy)])
            seen[sy][sx] = True
            area = 0
            while q:
                x, y = q.popleft()
                area += 1
                for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                    nx, ny = x + dx, y + dy
                    if 0 <= nx < w and 0 <= ny < h and not seen[ny][nx] and bg[ny][nx]:
                        seen[ny][nx] = True
                        q.append((nx, ny))
            if area >= min_area:
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


def path_stats(svg):
    """Zahl der Pfade und die höchste Befehlszahl eines einzelnen Pfades."""
    text = open(svg, encoding="utf-8").read()
    ds = re.findall(r'\sd\s*=\s*"([^"]*)"', text)
    if not ds:
        return 0, 0
    return len(ds), max(len(PATH_CMD.findall(d)) for d in ds)


def _luminance(hex_colour):
    hex_colour = hex_colour.lstrip("#")
    parts = [int(hex_colour[i:i + 2], 16) / 255 for i in (0, 2, 4)]
    lin = [c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4 for c in parts]
    return 0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]


def contrast(fg, bg):
    a, b = _luminance(fg), _luminance(bg)
    hi, lo = max(a, b), min(a, b)
    return (hi + 0.05) / (lo + 0.05)


def main():
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == "bg-components":
        print(bg_components(args[0]))
    elif cmd == "fill-ratio":
        fw, fh = fill_ratio(args[0])
        print(f"{fw:.4f} {fh:.4f}")
    elif cmd == "path-stats":
        n, m = path_stats(args[0])
        print(f"{n} {m}")
    elif cmd == "contrast":
        print(f"{contrast(args[0], args[1]):.2f}")
    else:
        raise SystemExit(f"unknown command: {cmd}")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Gate-Skript schreiben**

```bash
#!/usr/bin/env bash
# Logo-Gate: misst die Zeichnungen, statt sie zu begutachten.
#
# Die Kriterien stehen in
# docs/superpowers/specs/2026-08-03-owl-logo-monochrome-design.md.
# V2 ist der entscheidende Test: bleibt bei kleiner Rendergröße
# Negativraum übrig, oder wird die Marke zum Klumpen?
set -euo pipefail

repo_root=${LOGO_ARTWORK_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
cd "$repo_root"

measure="python3 scripts/lib/logo_measure.py"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail=0
ok()  { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

# Rendergröße und Pfadbudget je Stufe.
stage_size() { case $1 in full) echo 128;; reduced) echo 48;; micro) echo 16;; esac; }
stage_paths() { case $1 in full) echo 60;; reduced) echo 20;; micro) echo 1;; esac; }

# V1 ist nur auf quadratischen Rastern aussagekräftig. Die freistehende
# Marke definiert ihre eigenen Grenzen — dort ergäbe die Messung immer 1,0
# und würde Erfolg vortäuschen. Ihre Randfüllung wird auf den Zielflächen
# geprüft, die sie einbetten.
is_square_viewbox() {   # <svg>
  local vb; vb=$(grep -o 'viewBox="[^"]*"' "$1" | head -1 | tr -d 'viewBox="')
  local w h; w=$(echo "$vb" | awk '{print $3}'); h=$(echo "$vb" | awk '{print $4}')
  awk "BEGIN{exit !($w == $h)}"
}

check_v1() {   # <png> <stage> <svg>
  if ! is_square_viewbox "$3"; then
    ok "V1 übersprungen ($2 hat kein quadratisches Raster — auf den Zielflächen geprüft)"
    return
  fi
  read -r fw fh < <($measure fill-ratio "$1")
  if awk "BEGIN{exit !($fw >= 0.70 && $fh >= 0.70)}"; then
    ok "V1 Randfüllung $2: ${fw} × ${fh}"
  else
    bad "V1 Randfüllung $2: ${fw} × ${fh} — mindestens 0.70 in beiden Achsen"
  fi
}

check_v2() {   # <png> <label>
  local n; n=$($measure bg-components "$1")
  if [ "$n" -ge 2 ]; then
    ok "V2 Negativraum $2: $n Hintergrundkomponenten"
  else
    bad "V2 Negativraum $2: $n Komponente — die Aussparung ist zugelaufen"
  fi
}

check_v5() {   # <svg> <stage>
  read -r paths maxcmd < <($measure path-stats "$1")
  local budget; budget=$(stage_paths "$2")
  [ "$paths" -le "$budget" ] \
    && ok "V5 Pfadzahl $2: $paths ≤ $budget" \
    || bad "V5 Pfadzahl $2: $paths > $budget"
  [ "$maxcmd" -le 400 ] \
    && ok "V5 größter Pfad $2: $maxcmd Befehle" \
    || bad "V5 größter Pfad $2: $maxcmd Befehle > 400 — sieht nach Trace aus"
}

check_v4() {   # <hex>
  for ground in FFFFFF 1B082D; do
    local c; c=$($measure contrast "$1" "$ground")
    awk "BEGIN{exit !($c >= 4.5)}" \
      && ok "V4 Kontrast gegen #$ground: $c" \
      || bad "V4 Kontrast gegen #$ground: $c < 4.5"
  done
}

check_v7() {   # <svg>
  local paths; paths=$(grep -c '<path' "$1" || true)
  [ "$paths" -eq 1 ] && ok "V7 genau ein Pfad" || bad "V7 $paths Pfade, erwartet genau 1"
  grep -q 'viewBox="0 0 16 16"' "$1" && ok "V7 viewBox" || bad "V7 viewBox ist nicht 0 0 16 16"
  for forbidden in transform= stroke= linearGradient radialGradient; do
    grep -q "$forbidden" "$1" && bad "V7 enthält $forbidden" || ok "V7 ohne $forbidden"
  done
}

self_test() {
  # Klumpen: eine Hintergrundkomponente. Ring: zwei.
  # Die Fixtures sind SVG und laufen durch dieselbe Renderkette wie die
  # echten Zeichnungen — ein Fehler in rsvg-convert fiele hier ebenfalls auf.
  cat > "$tmp/blob.svg" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64"><path fill="#000000" d="M32 4a28 28 0 1 0 0.001 0z"/></svg>
EOF
  cat > "$tmp/ring.svg" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64"><path fill="#000000" fill-rule="evenodd" d="M32 4a28 28 0 1 0 0.001 0z M32 18a14 14 0 1 1 -0.001 0z"/></svg>
EOF
  rsvg-convert -w 64 -h 64 "$tmp/blob.svg" -o "$tmp/blob.png"
  rsvg-convert -w 64 -h 64 "$tmp/ring.svg" -o "$tmp/ring.png"
  local b r; b=$($measure bg-components "$tmp/blob.png"); r=$($measure bg-components "$tmp/ring.png")
  [ "$b" -eq 1 ] && ok "Selbsttest Klumpen = 1" || bad "Selbsttest Klumpen = $b, erwartet 1"
  [ "$r" -eq 2 ] && ok "Selbsttest Ring = 2"   || bad "Selbsttest Ring = $r, erwartet 2"
}

case ${1:-} in
  --self-test)
    echo "Kalibrierung des V2-Detektors"; self_test ;;
  --symbolic)
    svg=$2; echo "Symbolic: $svg"
    check_v7 "$svg"
    rsvg-convert -w 16 -h 16 "$svg" -o "$tmp/sym.png"
    check_v2 "$tmp/sym.png" "16px"
    # Kein Kontrasttest: GNOME färbt Symbolic-Icons zur Laufzeit mit der
    # Vordergrundfarbe des Themes um. Der literale Wert #222222 wird nie
    # angezeigt, ein Kontrastwert gegen irgendeinen Grund wäre bedeutungslos.
    # Was hier zählt, ist die Silhouette — und die prüft V2.
    ;;
  full|reduced|micro)
    stage=$1; svg=$2; size=$(stage_size "$stage")
    echo "Stufe $stage bei ${size}px: $svg"
    check_v5 "$svg" "$stage"
    rsvg-convert -w "$size" -h "$size" -a "$svg" -o "$tmp/s.png"
    check_v1 "$tmp/s.png" "$stage" "$svg"
    check_v2 "$tmp/s.png" "${size}px"
    # V3: alle Füllungen auf einen Wert abflachen, dann erneut V2
    sed -E 's/fill="(#[0-9A-Fa-f]{3,8}|url\([^)]*\))"/fill="#000000"/g' "$svg" > "$tmp/mono.svg"
    rsvg-convert -w "$size" -h "$size" -a "$tmp/mono.svg" -o "$tmp/m.png"
    check_v2 "$tmp/m.png" "${size}px monochrom" ;;
  *)
    echo "Aufruf: $0 {full|reduced|micro} <svg> | --symbolic <svg> | --self-test" >&2
    exit 2 ;;
esac

exit $fail
```

- [ ] **Step 3: Ausführbar machen und Selbsttest laufen lassen**

```bash
chmod +x scripts/check-logo-artwork.sh
./scripts/check-logo-artwork.sh --self-test
```

Erwartet: `ok Selbsttest Klumpen = 1` und `ok Selbsttest Ring = 2`, Exit 0.
Schlägt das fehl, ist der Detektor falsch und **jede** spätere Aussage wertlos —
erst hier weitermachen, wenn beide Zeilen grün sind.

- [ ] **Step 4: Gegen das heutige getracte Symbolic gegenprüfen**

Das ist die Realprobe: ein bekannt kaputtes Artefakt muss durchfallen.

```bash
git show HEAD:data/icons/hicolor/symbolic/apps/org.reprise.Reprise-symbolic.svg > /tmp/old-symbolic.svg
./scripts/check-logo-artwork.sh --symbolic /tmp/old-symbolic.svg || echo "erwartetes Scheitern"
```

Erwartet: mindestens ein `FAIL`. Das alte Symbolic auf `dev` ist das blaue
Vinyl-Icon mit mehreren Pfaden — V7 muss anschlagen. Meldet das Skript hier
Erfolg, prüft es nicht, was es soll.

- [ ] **Step 5: Commit**

```bash
git add scripts/check-logo-artwork.sh scripts/lib/logo_measure.py
git commit -m "build: gate logo drawings on measured negative space

Add the ruler before the artwork. The decisive check counts background
connected components in a small render: a mark whose cutouts close up
has exactly one, and that is precisely how the traced symbolic fails.

The detector calibrates itself against a synthetic blob and ring, so a
broken measurement surfaces as a failing self-test rather than as
silently green artwork."
```

---

### Task 2: Die `full`-Zeichnung

Vorlagentreu. Verläufe erwünscht.

**Files:**
- Create: `data/brand/mark.svg`
- Reference: `docs/assets/brand-reference/owl-headphones-template-clean.png`

**Interfaces:**
- Consumes: `scripts/check-logo-artwork.sh` aus Task 1.
- Produces: `data/brand/mark.svg` mit `viewBox="0 0 1247 1000"`, Gruppen-IDs
  `band`, `cups`, `tufts`, `head`, `face`, `brow`, `eyes`, `beak`. Task 5
  bettet die Datei als Ganzes ein, Task 8 leitet Mono- und Dunkelfassung
  daraus ab. Task 3 nimmt sie als Formvorlage, kopiert aber keine Pfade.

- [ ] **Step 1: Konstruktionsgerüst anlegen**

Startpunkt mit den gemessenen Landmarken. Die Kurven werden in Schritt 3
verfeinert; die Ankerpunkte stehen fest.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1247 1000" width="1247" height="1000">
  <defs>
    <linearGradient id="g-band" x1="0" y1="1" x2="0" y2="0">
      <stop offset="0" stop-color="#1F1056"/>
      <stop offset="0.55" stop-color="#4F93C8"/>
      <stop offset="1" stop-color="#8292D5"/>
    </linearGradient>
    <linearGradient id="g-head" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#2B155E"/>
      <stop offset="1" stop-color="#452674"/>
    </linearGradient>
    <radialGradient id="g-face" cx="0.5" cy="0.35" r="0.75">
      <stop offset="0" stop-color="#8A679C"/>
      <stop offset="1" stop-color="#5F2F8A"/>
    </radialGradient>
    <radialGradient id="g-iris" cx="0.5" cy="0.7" r="0.7">
      <stop offset="0" stop-color="#C7D98A"/>
      <stop offset="0.55" stop-color="#1C698A"/>
      <stop offset="1" stop-color="#114A71"/>
    </radialGradient>
  </defs>

  <!-- Bügel: Scheitel auf der Mittelachse y=0..145, Enden an den Muscheln -->
  <g id="band">
    <path d="M 96 674 C 96 262 330 40 623 40 C 916 40 1151 262 1151 674
             L 1071 674 C 1071 316 872 128 623 128 C 374 128 176 316 176 674 Z"
          fill="url(#g-band)"/>
  </g>

  <!-- Ohrmuscheln: breiteste Stelle der Marke, y=674 -->
  <g id="cups">
    <ellipse cx="118" cy="700" rx="118" ry="185" fill="#2B155E"/>
    <ellipse cx="1129" cy="700" rx="118" ry="185" fill="#2B155E"/>
    <ellipse cx="128" cy="700" rx="78" ry="150" fill="#5798BD"/>
    <ellipse cx="1119" cy="700" rx="78" ry="150" fill="#5798BD"/>
  </g>

  <!-- Ohrbüschel: Spitzen bei (179,200) und (1068,200), vor dem Bügel -->
  <g id="tufts">
    <path d="M 179 200 C 330 330 420 400 470 470 C 380 452 300 470 250 520 Z"
          fill="#2B155E"/>
    <path d="M 1068 200 C 917 330 827 400 777 470 C 867 452 947 470 997 520 Z"
          fill="#2B155E"/>
  </g>

  <!-- Kopf: Oberkante Mittelachse y=427, Kinn y=999 bei x=402..843 -->
  <g id="head">
    <path d="M 623 427 C 900 427 1010 600 1010 740 C 1010 900 843 999 623 999
             C 403 999 236 900 236 740 C 236 600 346 427 623 427 Z"
          fill="url(#g-head)"/>
  </g>

  <g id="face">
    <ellipse cx="623" cy="790" rx="300" ry="200" fill="url(#g-face)"/>
  </g>

  <!-- Winkelbrauen: dunkle V-Masse zwischen den Augen -->
  <g id="brow">
    <path d="M 623 560 L 900 660 L 880 720 L 623 700 L 366 720 L 346 660 Z"
          fill="#2B155E"/>
  </g>

  <g id="eyes">
    <ellipse cx="446" cy="757" rx="49" ry="50" fill="url(#g-iris)"/>
    <ellipse cx="799" cy="758" rx="49" ry="50" fill="url(#g-iris)"/>
    <circle cx="446" cy="757" r="24" fill="#1F1056"/>
    <circle cx="799" cy="758" r="24" fill="#1F1056"/>
    <circle cx="456" cy="742" r="8" fill="#FFFFFF"/>
    <circle cx="809" cy="743" r="8" fill="#FFFFFF"/>
  </g>

  <g id="beak">
    <path d="M 623 780 L 668 860 L 623 940 L 578 860 Z" fill="#A09EB4"/>
  </g>
</svg>
```

- [ ] **Step 2: Gegen die Vorlage stellen**

```bash
rsvg-convert -w 574 -h 460 -a data/brand/mark.svg -o /tmp/mark.png
magick docs/assets/brand-reference/owl-headphones-template-clean.png -resize 574x /tmp/ref.png
magick montage /tmp/ref.png /tmp/mark.png -tile 2x1 -geometry +8+8 -background '#d0d0d8' /tmp/compare.png
```

`/tmp/compare.png` ansehen. Die sechs Merkmale aus dem Spec-Abschnitt
„Vorlage" einzeln durchgehen: lange Büschel vor dem Bügel, Winkelbrauen,
Bügelverlauf, geriffelte Muscheln, Verlaufsaugen mit Lichtpunkt,
Gesichtsverlauf.

- [ ] **Step 3: Kurven verfeinern, bis alle sechs Merkmale sitzen**

Iterieren: Pfad ändern → rendern → vergleichen. Die Muschelriffelung als
konzentrische Ellipsen mit abnehmender Deckkraft ergänzen, die Büschelspitzen
schlanker ziehen, die Kopfkontur an die Vorlage anlegen. Der Vergleich aus
Schritt 2 ist nach jeder Änderung erneut zu erzeugen.

- [ ] **Step 4: Gate laufen lassen**

```bash
./scripts/check-logo-artwork.sh full data/brand/mark.svg
```

Erwartet: alle Zeilen `ok`, Exit 0. V1 muss ≥ 0.70 in beiden Achsen melden —
bei Seitenverhältnis 1,2467 auf quadratischem Render ergibt volle Breite rund
0.80 Höhe, also reichlich Luft. Schlägt V5 mit „sieht nach Trace aus" an, wurde
eine Vorlage importiert statt gezeichnet.

- [ ] **Step 5: Commit**

```bash
git add data/brand/mark.svg
git commit -m "feat: draw the full owl mark as vector

Hand-built from the measured landmarks of the cleaned reference rather
than traced from it: eye centres, tuft tips, chin and cup line come from
the template, the curves are authored.

Gradients stay in — the mark is the high-resolution face of the brand,
and SVG renders gradients at any density."
```

---

### Task 3: Die `reduced`-Zeichnung

Hier greift die geometrische Vereinfachung zum ersten Mal.

**Files:**
- Create: `data/brand/mark-reduced.svg`

**Interfaces:**
- Consumes: `data/brand/mark.svg` als Formvorlage.
- Produces: `data/brand/mark-reduced.svg`, `viewBox="0 0 64 64"`, ≤ 20 Pfade,
  keine Verläufe. Task 5 rendert daraus 48er und 64er PNGs, Task 6 leitet die
  Android-VectorDrawables ab.

- [ ] **Step 1: Zeichnung anlegen**

Quadratisches Raster, weil alle Zielflächen quadratisch sind. Die Marke wird
horizontal randfüllend gesetzt und vertikal zentriert. Muschelriffelung
entfällt, Verläufe werden Flächen.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64">
  <!-- Bügel: eine Masse, keine Glanzkante -->
  <path id="band" d="M 5 39 C 5 18 17 7 32 7 C 47 7 59 18 59 39
                     L 53 39 C 53 22 44 13 32 13 C 20 13 11 22 11 39 Z"
        fill="#4F93C8"/>
  <!-- Muscheln: schlichte Rundungen, gehen in die Kopfform über -->
  <ellipse id="cup-l" cx="8"  cy="40" rx="7" ry="11" fill="#2B155E"/>
  <ellipse id="cup-r" cx="56" cy="40" rx="7" ry="11" fill="#2B155E"/>
  <!-- Ohrbüschel: erhalten, sie tragen das Eulen-Signal -->
  <path id="tufts" d="M 11 12 C 18 20 22 24 25 28 C 21 27 17 28 15 31 Z
                      M 53 12 C 46 20 42 24 39 28 C 43 27 47 28 49 31 Z"
        fill="#2B155E"/>
  <path id="head" d="M 32 25 C 46 25 51 33 51 41 C 51 50 43 56 32 56
                     C 21 56 13 50 13 41 C 13 33 18 25 32 25 Z"
        fill="#2B155E"/>
  <ellipse id="face" cx="32" cy="44" rx="15" ry="11" fill="#5F2F8A"/>
  <path id="brow" d="M 32 32 L 47 37 L 46 41 L 32 40 L 18 41 L 17 37 Z"
        fill="#2B155E"/>
  <circle id="eye-l" cx="24" cy="43" r="4" fill="#1C698A"/>
  <circle id="eye-r" cx="40" cy="43" r="4" fill="#1C698A"/>
  <path id="beak" d="M 32 45 L 35 50 L 32 55 L 29 50 Z" fill="#A09EB4"/>
</svg>
```

- [ ] **Step 2: Gate laufen lassen**

```bash
./scripts/check-logo-artwork.sh reduced data/brand/mark-reduced.svg
```

Erwartet: alle `ok`. Der kritische Punkt ist V2 im Monochrom-Durchgang — wenn
Brauen, Augen und Gesicht zu einer Fläche verschmelzen, fällt die Zahl der
Hintergrundkomponenten auf 1.

- [ ] **Step 3: Bei V2-Verstoß die Augen vergrößern und die Brauen anheben**

Die Augen sind der Negativraum, der überleben muss. Radius von 4 auf 4.5
erhöhen und `brow` um 1 Einheit nach oben schieben, dann Schritt 2 wiederholen.
Nicht die Brauen entfernen — sie tragen das Eulen-Signal.

- [ ] **Step 4: Sichtprüfung bei Zielgröße**

```bash
for s in 48 64; do rsvg-convert -w $s -h $s -a data/brand/mark-reduced.svg -o /tmp/r$s.png; done
magick /tmp/r48.png -filter point -resize 384x384 /tmp/r48-zoom.png
magick /tmp/r64.png -filter point -resize 384x384 /tmp/r64-zoom.png
magick montage /tmp/r48-zoom.png /tmp/r64-zoom.png -tile 2x1 -geometry +8+8 -background '#d0d0d8' /tmp/reduced-check.png
```

- [ ] **Step 5: Commit**

```bash
git add data/brand/mark-reduced.svg
git commit -m "feat: draw the reduced owl mark for small surfaces

Cup ridging and gradients drop out because 48px cannot hold them; ear
tufts and angled brows stay because they are what reads as owl. The
square canvas matches every surface that consumes this drawing."
```

---

### Task 4: Die `micro`-Zeichnung — das Nadelöhr

An dieser Stufe scheitert das heutige Icon. Erst wenn sie besteht, geht
irgendetwas weiter.

**Files:**
- Create: `data/brand/mark-micro.svg`

**Interfaces:**
- Consumes: `data/brand/mark-reduced.svg` als Formvorlage.
- Produces: `data/brand/mark-micro.svg`, `viewBox="0 0 16 16"`, **genau ein
  `<path>`**, kein Bügel. Task 5 macht daraus das GNOME-Symbolic, Task 8 das
  Favicon.

- [ ] **Step 1: Zeichnung als Einzelpfad anlegen**

Live-Fläche 14×14 bei (1,1). Kein Bügel — bei 16 px wäre er eine 1-px-Linie,
die am Kopf klebt. Kopf, Büschel und Brauen tragen; Augen sind Aussparung und
werden über die Füllregel `evenodd` als Löcher im selben Pfad erzeugt.

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" width="16" height="16">
  <path fill="#222222" fill-rule="evenodd" d="M2 2 L6 6 L5 7 L4 6 L4 9
        C4 12 6 14 8 14 C10 14 12 12 12 9 L12 6 L11 7 L10 6 L14 2
        L13 7 L13 9 C13 12.5 10.8 15 8 15 C5.2 15 3 12.5 3 9 L3 7 Z
        M6 9 A1.1 1.1 0 1 0 6 11.2 A1.1 1.1 0 1 0 6 9 Z
        M10 9 A1.1 1.1 0 1 0 10 11.2 A1.1 1.1 0 1 0 10 9 Z"/>
</svg>
```

- [ ] **Step 2: Gate laufen lassen — das ist das Tor**

```bash
./scripts/check-logo-artwork.sh micro data/brand/mark-micro.svg
```

Erwartet: alle `ok`. Entscheidend sind zwei Zeilen:
- `V5 Pfadzahl micro: 1 ≤ 1`
- `V2 Negativraum 16px: 3 Hintergrundkomponenten` — Außenraum plus zwei Augen.

Meldet V2 nur 1, sind die Augen zugelaufen: Augenradius erhöhen, Abstand zur
Kopfkante vergrößern, Kanten auf ganze Pixel legen. **Nicht weitergehen**, bis
diese Zeile grün ist.

- [ ] **Step 3: Pixelraster prüfen**

```bash
rsvg-convert -w 16 -h 16 -a data/brand/mark-micro.svg -o /tmp/micro16.png
magick /tmp/micro16.png -filter point -resize 512x512 /tmp/micro-zoom.png
python3 -c "
from PIL import Image
im = Image.open('/tmp/micro16.png').convert('LA')
for y in range(16):
    print(''.join('#' if im.getpixel((x,y))[1] > 200 else ('+' if im.getpixel((x,y))[1] > 60 else '.') for x in range(16)))
"
```

Die ASCII-Ausgabe zeigt das echte Raster. Viele `+` bedeuten Zwischenwerte, also
Kanten neben dem Pixelraster — Koordinaten auf ganze oder halbe Einheiten
ziehen, bis überwiegend `#` und `.` stehen.

- [ ] **Step 4: Commit**

```bash
git add data/brand/mark-micro.svg
git commit -m "feat: draw the micro owl mark as a single path

No headband: at 16px it degenerates into a one-pixel line stuck to the
head. Head, tufts and brows carry the identity, and the eyes are holes in
the same path via evenodd so the silhouette survives monochrome flattening
with its cutouts intact."
```

---

### Task 5: GNOME-Flächen

**Files:**
- Create: `data/icons/hicolor/scalable/apps/org.reprise.Reprise.svg`
- Create: `data/icons/hicolor/{48x48,64x64,128x128,256x256,512x512}/apps/org.reprise.Reprise.png`
- Modify: `data/icons/hicolor/symbolic/apps/org.reprise.Reprise-symbolic.svg`
- Modify: `data/meson.build`

**Interfaces:**
- Consumes: `data/brand/mark.svg`, `mark-reduced.svg`, `mark-micro.svg`.
- Produces: installierte Icons unter `datadir/icons/hicolor`.

- [ ] **Step 1: Skalierbares App-Icon bauen**

128er Raster mit Grundfläche. Die Marke wird auf die Grundfläche eingepasst:
Breite 96 bei Seitenverhältnis 1,2467 ergibt Höhe 77, zentriert bei (16, 25.5).
Der Skalierungsfaktor ist `96 / 1247 = 0.07699`.

Das Einbetten wird geskriptet, nicht von Hand kopiert — sonst driftet das
App-Icon von `mark.svg` ab, sobald die Marke nachgezogen wird.

```bash
mkdir -p data/icons/hicolor/scalable/apps
python3 - <<'PY'
import re
mark = open('data/brand/mark.svg', encoding='utf-8').read()
inner = re.sub(r'^.*?<svg[^>]*>', '', mark, flags=re.S)
inner = re.sub(r'</svg>\s*$', '', inner, flags=re.S)
out = (
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" '
    'width="128" height="128">\n'
    '  <rect x="8" y="8" width="112" height="112" rx="26" fill="#1B082D"/>\n'
    '  <g transform="translate(16 25.5) scale(0.07699)">\n'
    f'{inner}\n'
    '  </g>\n'
    '</svg>\n'
)
open('data/icons/hicolor/scalable/apps/org.reprise.Reprise.svg', 'w',
     encoding='utf-8').write(out)
print("geschrieben")
PY
rsvg-convert -w 128 -h 128 data/icons/hicolor/scalable/apps/org.reprise.Reprise.svg -o /tmp/appicon.png
python3 scripts/lib/logo_measure.py fill-ratio /tmp/appicon.png
```

Erwartet: `geschrieben`, danach zwei Werte ≥ 0.90 — das App-Icon ist
randfüllend, weil die Grundfläche mitzählt. Hier greift V1 auf der Zielfläche,
die bei `mark.svg` selbst übersprungen wurde.

- [ ] **Step 2: Symbolic aus `micro` erzeugen**

```bash
cp data/brand/mark-micro.svg data/icons/hicolor/symbolic/apps/org.reprise.Reprise-symbolic.svg
./scripts/check-logo-artwork.sh --symbolic data/icons/hicolor/symbolic/apps/org.reprise.Reprise-symbolic.svg
```

Erwartet: alle `ok`, Exit 0. Genau der Aufruf, der in Task 1 Schritt 4 am alten
Icon scheiterte.

Falls `micro` mehr als einen Pfad hätte, vereinigt Inkscape sie:

```bash
inkscape --actions="select-all;path-union;export-plain-svg;export-filename:/tmp/union.svg;export-do" data/brand/mark-micro.svg
```

Ohne Inkscape: die Teilpfade von Hand in ein `d`-Attribut zusammenziehen und
`fill-rule="evenodd"` setzen — bei der Konstruktion aus Task 4 ist das bereits
der Fall.

- [ ] **Step 3: PNG-Stufen rendern**

48 und 64 kommen aus `reduced`, die großen aus `full`. Genau das ist der Sinn
der größenabhängigen Zeichnungen.

```bash
set -e
for s in 48 64; do
  mkdir -p "data/icons/hicolor/${s}x${s}/apps"
  rsvg-convert -w $s -h $s -a data/brand/mark-reduced.svg \
    -o "data/icons/hicolor/${s}x${s}/apps/org.reprise.Reprise.png"
done
for s in 128 256 512; do
  mkdir -p "data/icons/hicolor/${s}x${s}/apps"
  rsvg-convert -w $s -h $s data/icons/hicolor/scalable/apps/org.reprise.Reprise.svg \
    -o "data/icons/hicolor/${s}x${s}/apps/org.reprise.Reprise.png"
done
identify data/icons/hicolor/*/apps/org.reprise.Reprise.png
```

Erwartet: fünf Zeilen mit den jeweils passenden Kantenlängen.

- [ ] **Step 4: meson wieder auf das skalierbare Icon zeigen lassen**

In `data/meson.build` den Kommentar korrigieren und die SVG-Installation
ergänzen. Die PNG-Schleife bleibt.

```meson
# Das App-Icon liegt skalierbar vor; die Sonderstufen 48 und 64 kommen aus
# einer eigenen, vereinfachten Zeichnung und werden deshalb zusätzlich als
# PNG installiert.
install_data(
  'icons/hicolor/scalable/apps/' + app_id + '.svg',
  install_dir: get_option('datadir') / 'icons/hicolor/scalable/apps',
)

foreach size : ['48x48', '64x64', '128x128', '256x256', '512x512']
  install_data(
    'icons/hicolor/' + size + '/apps/' + app_id + '.png',
    install_dir: get_option('datadir') / 'icons/hicolor' / size / 'apps',
  )
endforeach
```

- [ ] **Step 5: Konfiguration prüfen**

```bash
meson setup /tmp/build-icons . >/dev/null && echo "meson ok"
```

Erwartet: `meson ok`. Fehlt eine der Dateien, bricht meson mit dem Pfad ab, der
fehlt.

- [ ] **Step 6: Commit**

```bash
git add data/icons data/meson.build
git commit -m "feat: ship the owl icon scalable again, with sized stages

Restore the scalable SVG the raster switch dropped — its stated reason
was that gradient artwork forces raster, which SVG disproves. The PNG
stages stay, but now they carry real size-specific drawings: 48 and 64
render from the reduced drawing, the larger ones from the full one.

The symbolic icon becomes the micro drawing: one path on a 16px grid,
replacing an autotraced path declared at 16x16 with a 521-unit viewBox."
```

---

### Task 6: Android-Flächen

**Files:**
- Create: `scripts/lib/svg_to_vectordrawable.py`
- Create: `android/app/src/main/res/drawable/ic_launcher_foreground.xml`
- Create: `android/app/src/main/res/drawable/ic_launcher_monochrome.xml`
- Create: `android/app/src/main/res/values/ic_launcher_background.xml`
- Create: `android/app/src/main/res/mipmap-anydpi-v26/ic_launcher.xml`
- Create: `android/app/src/main/res/mipmap-anydpi-v26/ic_launcher_round.xml`
- Create: `data/brand/play-store-icon-512.png`
- Modify: `android/app/src/main/AndroidManifest.xml`

**Interfaces:**
- Consumes: `data/brand/mark-reduced.svg`.
- Produces: `@mipmap/ic_launcher`, `@mipmap/ic_launcher_round`.

VectorDrawable ist **nicht** SVG: kein `<ellipse>`, kein `<circle>`, keine
`fill-rule`, keine Verläufe über `url(#…)` in dieser Form. Alle Formen müssen
als `<path android:pathData="…">` vorliegen. Die Zeichnung aus Task 3 wird
deshalb umgeschrieben, nicht kopiert.

- [ ] **Step 1: Konverter schreiben**

VectorDrawable kennt weder `<ellipse>` noch `<circle>` noch `fill-rule`. Der
Konverter übersetzt die Formen aus `mark-reduced.svg` und rechnet dabei vom
64er Raster auf das 108er um, so dass die Marke exakt in der 72-dp-Safe-Zone
liegt: Faktor `72/64 = 1.125`, Versatz `(108−72)/2 = 18`.

Create: `scripts/lib/svg_to_vectordrawable.py`

```python
#!/usr/bin/env python3
"""Übersetzt die reduzierte Zeichnung in einen Android VectorDrawable.

Nötig, weil VectorDrawable nur <path> kennt: Ellipsen, Kreise und
fill-rule müssen vorher aufgelöst werden. Die Skalierung setzt die Marke
in die 72dp-Safe-Zone des 108dp-Viewports.
"""
import re
import sys

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
    for token in re.findall(r"[MLCZ]|-?[\d.]+", d):
        if token in "MLCZ":
            out.append(token)
        else:
            out.append(f"{float(token) * SCALE + OFFSET:.3f}")
    return " ".join(out)


def convert(src, mono):
    text = open(src, encoding="utf-8").read()
    shapes = []
    for m in re.finditer(r"<(path|ellipse|circle)\b([^>]*)/?>", text):
        kind, attrs = m.group(1), m.group(2)
        fill = (re.search(r'fill="([^"]+)"', attrs) or [None, "#000000"])[1]
        if kind == "path":
            d = re.search(r'\sd="([^"]+)"', attrs).group(1)
            shapes.append((scale_path(d), fill))
        else:
            g = lambda k, dflt=None: float(
                (re.search(rf'{k}="([^"]+)"', attrs) or [None, dflt])[1])
            if kind == "circle":
                r = g("r")
                shapes.append((ellipse_path(g("cx"), g("cy"), r, r), fill))
            else:
                shapes.append((ellipse_path(g("cx"), g("cy"), g("rx"), g("ry")), fill))

    head = ('<?xml version="1.0" encoding="utf-8"?>\n'
            '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
            '    android:width="108dp"\n'
            '    android:height="108dp"\n'
            '    android:viewportWidth="108"\n'
            '    android:viewportHeight="108">\n')
    if mono:
        # Eine Fläche, Augen als Löcher. Das System tönt den Layer; nur Alpha zählt.
        merged = " ".join(d for d, _ in shapes)
        body = ('    <path android:fillColor="#000000" android:fillType="evenOdd"\n'
                f'          android:pathData="{merged}"/>\n')
    else:
        body = "".join(
            f'    <path android:fillColor="{f}" android:pathData="{d}"/>\n'
            for d, f in shapes)
    return head + body + "</vector>\n"


if __name__ == "__main__":
    src, dest = sys.argv[1], sys.argv[2]
    open(dest, "w", encoding="utf-8").write(convert(src, mono="monochrome" in dest))
    print("geschrieben", dest)
```

- [ ] **Step 2: Beide Layer erzeugen**

```bash
mkdir -p android/app/src/main/res/drawable android/app/src/main/res/mipmap-anydpi-v26
python3 scripts/lib/svg_to_vectordrawable.py data/brand/mark-reduced.svg \
  android/app/src/main/res/drawable/ic_launcher_foreground.xml
python3 scripts/lib/svg_to_vectordrawable.py data/brand/mark-reduced.svg \
  android/app/src/main/res/drawable/ic_launcher_monochrome.xml
grep -c '<path' android/app/src/main/res/drawable/ic_launcher_foreground.xml
grep -c '<path' android/app/src/main/res/drawable/ic_launcher_monochrome.xml
```

Erwartet: `geschrieben …` zweimal, danach `10` für den Foreground und `1` für
den Monochrome-Layer.

Bricht der Konverter mit „relative Pfadbefehle" ab, enthält
`mark-reduced.svg` klein geschriebene Pfadbefehle — die Zeichnung aus Task 3
nutzt bewusst nur absolute.

- [ ] **Step 3: Safe-Zone rechnerisch nachweisen**

```bash
python3 - <<'PY'
import re
d = open('android/app/src/main/res/drawable/ic_launcher_foreground.xml',
         encoding='utf-8').read()
vals = [float(v) for v in re.findall(r'-?\d+\.\d+', d)]
xs, ys = vals[0::2], vals[1::2]
print(f"x {min(xs):.1f}..{max(xs):.1f}   y {min(ys):.1f}..{max(ys):.1f}")
print("Safe-Zone 18..90 eingehalten:", min(xs) >= 17.5 and max(xs) <= 90.5)
PY
```

Erwartet: `Safe-Zone 18..90 eingehalten: True`. Das ist die rechnerische
Vorprüfung zu V6 — steht sie auf `False`, wird der Launcher die Marke
anschneiden, und der Emulatorlauf in Schritt 7 kann das nur noch bestätigen.

- [ ] **Step 4: Hintergrundfarbe und Adaptive-Icon-XML**

`values/ic_launcher_background.xml`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<resources>
    <color name="ic_launcher_background">#1B082D</color>
</resources>
```

`mipmap-anydpi-v26/ic_launcher.xml` und `ic_launcher_round.xml` — beide
identisch, die Maske übernimmt der Launcher:

```xml
<?xml version="1.0" encoding="utf-8"?>
<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">
    <background android:drawable="@color/ic_launcher_background"/>
    <foreground android:drawable="@drawable/ic_launcher_foreground"/>
    <monochrome android:drawable="@drawable/ic_launcher_monochrome"/>
</adaptive-icon>
```

- [ ] **Step 5: Manifest verdrahten**

In `android/app/src/main/AndroidManifest.xml` das `<application>`-Element
ergänzen:

```xml
    <application
        android:icon="@mipmap/ic_launcher"
        android:roundIcon="@mipmap/ic_launcher_round"
        android:label="Reprise Android MVP"
        android:supportsRtl="true"
        android:theme="@style/Theme.Spike">
```

- [ ] **Step 6: Bauen**

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
cd android && ./gradlew :app:assembleDebug
```

Erwartet: `BUILD SUCCESSFUL`. Bricht `aapt2` mit einem Pfaddatenfehler ab,
enthält eine `pathData` noch SVG-Syntax, die VectorDrawable nicht kennt —
typischerweise `fill-rule` statt `android:fillType` oder ein `url(#…)`.

- [ ] **Step 7: V6 auf dem Emulator prüfen**

```bash
"$HOME/Android/Sdk/emulator/emulator" -avd pixel10xl_api37 -no-window -no-audio &
adb wait-for-device
adb install -r android/app/build/outputs/apk/debug/app-debug.apk
# Themed Icons einschalten und Launcher-Icon abgreifen
adb shell settings put secure icon_pack_theme 1 || true
adb shell screencap -p /sdcard/launcher.png && adb pull /sdcard/launcher.png /tmp/launcher.png
```

`/tmp/launcher.png` ansehen: der Monochrome-Layer muss sichtbar sein und darf
von der Launcher-Maske nicht angeschnitten werden. Wird die Marke beschnitten,
liegt sie außerhalb der 72-dp-Safe-Zone — Skalierungsfaktor in Schritt 1
verkleinern.

Der Emulator läuft mit `-no-window`; es erscheint kein Fenster auf dem Desktop.

- [ ] **Step 8: Play-Store-Icon rendern**

```bash
rsvg-convert -w 512 -h 512 data/icons/hicolor/scalable/apps/org.reprise.Reprise.svg \
  -o data/brand/play-store-icon-512.png
identify data/brand/play-store-icon-512.png
```

Erwartet: `512x512`.

- [ ] **Step 9: Commit**

```bash
git add android/app/src/main/res android/app/src/main/AndroidManifest.xml data/brand/play-store-icon-512.png
git commit -m "feat: give the Android app a launcher icon

The app shipped with no android:icon at all, so every launcher drew the
platform default. Add an adaptive icon with a monochrome layer for the
themed icons of API 33 and up.

minSdk 26 means adaptive icons are guaranteed, so no legacy density
buckets are needed. The drawings are rewritten as VectorDrawable paths
rather than copied — that format takes no ellipse, circle or fill-rule."
```

---

### Task 7: Wortmarke beschaffen und in Pfade legen

**Files:**
- Create: `data/brand/fonts/Fraunces-SemiBold.ttf`
- Create: `scripts/lib/wordmark_to_path.py`

**Interfaces:**
- Produces: `python3 scripts/lib/wordmark_to_path.py <ttf> <text> <size>` gibt
  ein `d`-Attribut und die Vorschubbreite auf stdout aus. Task 8 verwendet das
  für die Outlined-Lockups.

- [ ] **Step 1: Fraunces holen**

```bash
mkdir -p data/brand/fonts
curl -fsSL -o /tmp/fraunces.ttf \
  "https://github.com/google/fonts/raw/main/ofl/fraunces/Fraunces%5BSOFT%2CWONK%2Copsz%2Cwght%5D.ttf"
python3 -c "
from fontTools.ttLib import TTFont
f = TTFont('/tmp/fraunces.ttf')
print('Achsen:', [(a.axisTag, a.minValue, a.defaultValue, a.maxValue) for a in f['fvar'].axes])
"
```

Erwartet: die Achsen `SOFT`, `WONK`, `opsz`, `wght`. Kommt ein 404, ist der
Dateiname im Repo geändert worden — dann `https://fonts.google.com/specimen/Fraunces`
öffnen und die statische SemiBold ziehen.

- [ ] **Step 2: Auf eine statische Instanz festlegen**

Display-Größe, halbfett, leicht eigenwillig — die Einstellung, die den
Schriftzug tragen soll.

```bash
python3 - <<'PY'
from fontTools.varLib.instancer import instantiateVariableFont
from fontTools.ttLib import TTFont
f = TTFont('/tmp/fraunces.ttf')
inst = instantiateVariableFont(f, {"wght": 600, "opsz": 144, "SOFT": 20, "WONK": 1})
inst.save('data/brand/fonts/Fraunces-SemiBold.ttf')
print("gespeichert")
PY
ls -la data/brand/fonts/
```

- [ ] **Step 3: Text-nach-Pfad-Werkzeug schreiben**

```python
#!/usr/bin/env python3
"""Setzt einen kurzen Schriftzug als SVG-Pfaddaten.

Wird für die Outlined-Lockups gebraucht: Live-Text bricht ohne geladene
Schrift, die Pfadfassung nicht.
"""
import sys

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.ttLib import TTFont


def wordmark(ttf_path, text, size):
    font = TTFont(ttf_path)
    glyphs = font.getGlyphSet()
    cmap = font.getBestCmap()
    upem = font["head"].unitsPerEm
    scale = size / upem
    parts, x = [], 0.0
    for ch in text:
        name = cmap[ord(ch)]
        pen = SVGPathPen(glyphs)
        glyphs[name].draw(pen)
        d = pen.getCommands()
        if d:
            parts.append(f'<path transform="translate({x * scale:.3f} 0) '
                         f'scale({scale:.6f} {-scale:.6f})" d="{d}"/>')
        x += glyphs[name].width
    return "\n".join(parts), x * scale


if __name__ == "__main__":
    paths, width = wordmark(sys.argv[1], sys.argv[2], float(sys.argv[3]))
    print(paths)
    print(f"<!-- Vorschubbreite: {width:.2f} -->", file=sys.stderr)
```

Der negative Y-Faktor kippt das Schriftkoordinatensystem, dessen Y-Achse nach
oben zeigt, in das von SVG.

- [ ] **Step 4: Probe erzeugen**

```bash
python3 scripts/lib/wordmark_to_path.py data/brand/fonts/Fraunces-SemiBold.ttf Reprise 100 > /tmp/word.frag 2>/tmp/word.width
cat /tmp/word.width
{ echo '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 -80 460 120" width="460">'
  cat /tmp/word.frag; echo '</svg>'; } > /tmp/word.svg
rsvg-convert -w 460 -b white /tmp/word.svg -o /tmp/word.png
identify /tmp/word.png
```

`/tmp/word.png` ansehen: „Reprise" muss vollständig und korrekt sitzen. Ist die
Zeile leer, stimmt die `viewBox`-Verschiebung nicht — der Text sitzt auf der
Grundlinie bei y=0 und reicht nach oben.

- [ ] **Step 5: Commit**

```bash
git add data/brand/fonts scripts/lib/wordmark_to_path.py
git commit -m "feat: pin the wordmark typeface and outline it

Fraunces at a display optical size carries the wordmark: its editorial
warmth is the deliberate counterweight to the geometric owl, and a
variable axis set lets the instance be tuned rather than merely enlarged.

Ship a pinned static instance plus a converter, so the outlined lockups
render identically where the webfont is unavailable."
```

---

### Task 8: Lockups, Web-Varianten und Favicon-Satz

**Files:**
- Create: `data/brand/mark-mono.svg`, `data/brand/mark-on-dark.svg`
- Create: `data/brand/lockup-horizontal.svg`, `data/brand/lockup-vertical.svg`
- Create: `data/brand/lockup-horizontal-outlined.svg`, `data/brand/lockup-vertical-outlined.svg`
- Create: `data/brand/favicon.svg`, `data/brand/favicon-32.png`, `data/brand/apple-touch-icon-180.png`

**Interfaces:**
- Consumes: `mark.svg`, `mark-micro.svg`, `wordmark_to_path.py` aus Task 7.

- [ ] **Step 1: Mono- und Dunkelfassung ableiten**

```bash
# Mono: alle Füllungen auf currentColor, Verläufe entfernen
python3 - <<'PY'
import re
src = open('data/brand/mark.svg', encoding='utf-8').read()
mono = re.sub(r'<defs>.*?</defs>', '', src, flags=re.S)
mono = re.sub(r'fill="(#[0-9A-Fa-f]{3,8}|url\([^)]*\))"', 'fill="currentColor"', mono)
open('data/brand/mark-mono.svg', 'w', encoding='utf-8').write(mono)

# Dunkelfassung: Körperwerte anheben, Augen-Akzent bleibt hellster Wert
lift = {'#2B155E': '#5B3E93', '#452674': '#7A56B0', '#5F2F8A': '#9A78C6',
        '#8A679C': '#C2A8D4', '#1F1056': '#3A2470'}
dark = src
for a, b in lift.items():
    dark = dark.replace(a, b)
open('data/brand/mark-on-dark.svg', 'w', encoding='utf-8').write(dark)
print("geschrieben")
PY
./scripts/check-logo-artwork.sh full data/brand/mark-mono.svg
```

Erwartet: alle `ok`. V3 prüft die Monochromfassung ohnehin, aber die eigene
Datei muss für sich bestehen.

- [ ] **Step 2: Kontrast der Dunkelfassung nachweisen**

```bash
for c in 5B3E93 7A56B0 9A78C6 C2A8D4; do
  printf "%s gegen #1B082D: " "$c"
  python3 scripts/lib/logo_measure.py contrast "$c" 1B082D
done
```

Erwartet: der hellste Wert ≥ 4.5. Reicht keiner, die Ersetzungstabelle in
Schritt 1 weiter anheben.

- [ ] **Step 3: Beide Lockups mit Live-Text erzeugen**

Horizontal: Markenhöhe 80, also Breite 99,7 bei Seitenverhältnis 1,2467,
Skalierung `80/1000 = 0.08`. Vertikal: Markenhöhe 124, Skalierung
`124/1000 = 0.124`, Breite 154,6, also linksbündig bei `(260−154.6)/2 = 52.7`.

Wie in Task 5 wird die Marke geskriptet eingebettet, damit die Lockups nicht
von `mark.svg` abdriften.

```bash
python3 - <<'PY'
import re
mark = open('data/brand/mark.svg', encoding='utf-8').read()
inner = re.sub(r'^.*?<svg[^>]*>', '', mark, flags=re.S)
inner = re.sub(r'</svg>\s*$', '', inner, flags=re.S)
FONT = "Fraunces, 'Instrument Serif', Georgia, serif"

horizontal = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 420 100" width="420" height="100">
  <g transform="translate(0 10) scale(0.08)">
{inner}
  </g>
  <text x="132" y="72" font-family="{FONT}"
        font-size="58" font-weight="600" letter-spacing="-1.5"
        fill="currentColor">Reprise</text>
</svg>
'''

vertical = f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 260 200" width="260" height="200">
  <g transform="translate(52.7 8) scale(0.124)">
{inner}
  </g>
  <text x="130" y="180" text-anchor="middle" font-family="{FONT}"
        font-size="44" font-weight="600" letter-spacing="-1"
        fill="currentColor">Reprise</text>
</svg>
'''

open('data/brand/lockup-horizontal.svg', 'w', encoding='utf-8').write(horizontal)
open('data/brand/lockup-vertical.svg', 'w', encoding='utf-8').write(vertical)
print("geschrieben")
PY
rsvg-convert -w 420 -b white data/brand/lockup-horizontal.svg -o /tmp/lk-h.png
rsvg-convert -w 260 -b white data/brand/lockup-vertical.svg -o /tmp/lk-v.png
identify /tmp/lk-h.png /tmp/lk-v.png
```

Erwartet: `geschrieben`, danach `420x100` und `260x200`.

- [ ] **Step 4: Schriftgröße gegen die Marke ausrichten**

`/tmp/lk-h.png` ansehen. Die Versalhöhe von „Reprise" soll etwa zwei Dritteln
der Markenhöhe entsprechen; ragt der Schriftzug über die Marke hinaus oder
wirkt verloren, `font-size` in Schritt 3 anpassen und erneut ausführen. Die
Startwerte 58 und 44 sind auf Fraunces bei diesem Raster gerechnet, aber
Schriftmetrik ist nichts, was man blind übernimmt.

- [ ] **Step 5: Outlined-Fassungen erzeugen**

```bash
for v in horizontal vertical; do
  cp "data/brand/lockup-$v.svg" "data/brand/lockup-$v-outlined.svg"
done
python3 - <<'PY'
import re, subprocess
# Die Größen müssen mit denen aus Schritt 3 übereinstimmen, sonst sitzt
# die Pfadfassung anders als die Live-Text-Fassung.
for variant, size, x, y, anchor in (("horizontal", 58, 132, 72, False),
                                    ("vertical", 44, 130, 180, True)):
    frag = subprocess.run(
        ["python3", "scripts/lib/wordmark_to_path.py",
         "data/brand/fonts/Fraunces-SemiBold.ttf", "Reprise", str(size)],
        capture_output=True, text=True, check=True)
    paths, width = frag.stdout, None
    m = re.search(r"([\d.]+)", frag.stderr)
    width = float(m.group(1)) if m else 0.0
    ox = x - (width / 2 if anchor else 0)
    path = f"data/brand/lockup-{variant}-outlined.svg"
    src = open(path, encoding="utf-8").read()
    src = re.sub(r"<text.*?</text>",
                 f'<g fill="currentColor" transform="translate({ox:.2f} {y})">\n{paths}</g>',
                 src, flags=re.S)
    open(path, "w", encoding="utf-8").write(src)
    print("geschrieben", path)
PY
```

- [ ] **Step 6: Beide Fassungen gegeneinander rendern**

```bash
for f in horizontal horizontal-outlined vertical vertical-outlined; do
  rsvg-convert -w 420 -b white "data/brand/lockup-$f.svg" -o "/tmp/lk-$f.png"
done
magick montage /tmp/lk-horizontal.png /tmp/lk-horizontal-outlined.png \
               /tmp/lk-vertical.png /tmp/lk-vertical-outlined.png \
  -tile 2x2 -geometry +10+10 -background '#d0d0d8' /tmp/lockups.png
```

`/tmp/lockups.png` ansehen: Live-Text- und Pfadfassung müssen deckungsgleich
sitzen. Weichen sie ab, ist Fraunces im System installiert und `rsvg-convert`
setzt die Live-Fassung anders als die gepinnte Instanz — dann ist die
Pfadfassung maßgeblich.

- [ ] **Step 7: Favicon-Satz**

Das Favicon kommt aus `micro`, nicht aus der vollen Marke — Browser-Tabs
rendern 16 px.

```bash
cp data/brand/mark-micro.svg data/brand/favicon.svg
rsvg-convert -w 32 -h 32 -a data/brand/mark-micro.svg -o data/brand/favicon-32.png
rsvg-convert -w 180 -h 180 data/icons/hicolor/scalable/apps/org.reprise.Reprise.svg \
  -o data/brand/apple-touch-icon-180.png
identify data/brand/favicon-32.png data/brand/apple-touch-icon-180.png
```

Erwartet: `32x32` und `180x180`.

- [ ] **Step 8: Gesamtabnahme**

```bash
set -e
./scripts/check-logo-artwork.sh --self-test
./scripts/check-logo-artwork.sh full data/brand/mark.svg
./scripts/check-logo-artwork.sh full data/brand/mark-mono.svg
./scripts/check-logo-artwork.sh full data/brand/mark-on-dark.svg
./scripts/check-logo-artwork.sh reduced data/brand/mark-reduced.svg
./scripts/check-logo-artwork.sh micro data/brand/mark-micro.svg
./scripts/check-logo-artwork.sh --symbolic data/icons/hicolor/symbolic/apps/org.reprise.Reprise-symbolic.svg
echo "alle Stufen bestanden"
```

Erwartet: `alle Stufen bestanden`. `set -e` sorgt dafür, dass der erste
Verstoß abbricht.

- [ ] **Step 9: Commit**

```bash
git add data/brand
git commit -m "feat: complete the web brand set

Mark in colour, on-dark and monochrome, both lockups as live text and as
outlines, and a favicon that comes from the micro drawing rather than the
full mark — browser tabs render 16px, and shipping the detailed mark
there is how logos turn to mush."
```

---

## Abnahme

Der Plan ist fertig, wenn Task 8 Schritt 8 durchläuft und zusätzlich gilt:

- `meson setup` konfiguriert (Task 5 Schritt 5).
- `./gradlew :app:assembleDebug` baut (Task 6 Schritt 6).
- V6 auf dem Emulator geprüft (Task 6 Schritt 7) — das einzige Kriterium, das
  sich nicht aus Dateien allein belegen lässt.
- `git log --oneline` zeigt acht Commits, einen je Task.
