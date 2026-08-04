#!/usr/bin/env bash
# Logo-Gate: misst die Zeichnungen, statt sie zu begutachten.
#
# Die Kriterien stehen in
# docs/superpowers/specs/2026-08-03-owl-logo-monochrome-design.md.
# V2 ist der entscheidende Test: bleibt bei kleiner Rendergröße
# Negativraum übrig, oder wird die Marke zum Klumpen?
#
#   ./scripts/check-logo-artwork.sh --all        alles, was ausgeliefert wird
#   ./scripts/check-logo-artwork.sh full <svg>   eine einzelne Stufe
set -euo pipefail

repo_root=${LOGO_ARTWORK_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
cd "$repo_root"

measure="python3 scripts/lib/logo_measure.py"
flatten="python3 scripts/lib/svg_flatten.py"
layer="python3 scripts/lib/svg_layer.py"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail=0
ok()  { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

# Es gibt nur noch eine Zeichnung. Sie wird bei der kleinsten Größe geprüft,
# bei der sie ausgeliefert wird — was dort trägt, trägt auch darüber.
MARK_SIZE=16
MARK_SHAPES=12

# Kontrastschwellen. 3,0 ist WCAG 1.4.11 für grafische Objekte — die Grenze,
# an der eine Fläche sich von ihrem Grund abhebt. Die frühere Fassung
# verlangte 4,5, den Wert für Fließtext, und zwar von **jedem** Palettenwert
# gegen Weiß und Schwarz. Das ist für eine Zeichnung, die auf ihrer eigenen
# Platte sitzt, kein sinnvoller Test: er verbot helle Glanzpunkte und sagte
# nichts darüber, ob die Marke auf ihrer Platte steht.
MIN_EDGE=3.0
MAX_BLIND=0.02      # Anteil der Fläche, der unter 1,5:1 im Grund versinkt

is_square_viewbox() {   # <svg>
  local vb w h
  vb=$(grep -o 'viewBox="[^"]*"' "$1" | head -1 | sed 's/viewBox="//; s/"//')
  w=$(echo "$vb" | awk '{print $3}'); h=$(echo "$vb" | awk '{print $4}')
  awk "BEGIN{exit !($w == $h)}"
}

check_v1() {   # <png> <stage> <svg>
  if ! is_square_viewbox "$3"; then
    ok "V1 übersprungen ($2 hat kein quadratisches Raster — auf den Zielflächen geprüft)"
    return
  fi
  read -r fw fh < <($measure fill-ratio "$1")
  awk "BEGIN{exit !($fw >= 0.70 && $fh >= 0.70)}" \
    && ok "V1 Randfüllung $2: ${fw} × ${fh}" \
    || bad "V1 Randfüllung $2: ${fw} × ${fh} — mindestens 0.70 in beiden Achsen"
}

check_v2() {   # <png> <label>
  local n; n=$($measure bg-components "$1")
  [ "$n" -ge 2 ] \
    && ok "V2 Negativraum $2: $n Hintergrundkomponenten" \
    || bad "V2 Negativraum $2: $n Komponente — die Aussparung ist zugelaufen"
}

check_v3() {   # <svg> <stage> <size>
  $flatten "$1" "$tmp/mono.svg"
  rsvg-convert -w "$3" -h "$3" -a "$tmp/mono.svg" -o "$tmp/mono.png"
  check_v2 "$tmp/mono.png" "${3}px einfarbig"
}

# Zur Erinnerung, warum hier kein Augen-Zähler steht: der Versuch, „bleiben
# bei 48 px zwei Augen übrig" über zusammenhängende Farbflächen zu messen,
# war nicht stabil. Auf der Verlaufs-Iris zerfällt jedes Auge in mehrere
# Teilflächen, und die Zahl schwankt mit der Rendergröße zwischen 2 und 9 —
# ein Gate, das mal grün und mal rot wird, ohne dass sich die Zeichnung
# ändert, ist schlimmer als keins. Was den farbigen Stufen bleibt, ist V4:
# die Marke muss auf ihrer Platte stehen. Ob sie innen liest, ist an
# gerenderten Bildern entschieden und in der Spec begründet.

# V4 misst am gerenderten Bild, nicht an Hex-Werten aus der Datei. Was
# entscheidet, ob eine Marke auf einem Grund steht, ist ihr Saum: das Innere
# darf beliebig hell sein, solange die Außenkante trägt.
report_contrast() {   # <median> <min> <anteil3> <blind> <label>
  awk -v m="$1" -v blind="$4" -v lim="$MIN_EDGE" -v maxblind="$MAX_BLIND" \
      "BEGIN{exit !(m >= lim && blind <= maxblind)}" \
    && ok "V4 $5: Median $1, blind $4" \
    || bad "V4 $5: Median $1 (< $MIN_EDGE) oder blind $4 (> $MAX_BLIND)"
}

check_v4_ground() {   # <png> <hex> <label>
  report_contrast $($measure edge-contrast "$1" "$2") "$3"
}

check_v4_plate() {   # <zusammengesetztes-icon.svg> <label>
  # Marke und Platte einzeln rendern und Pixel gegen Pixel halten. Ein
  # Verlauf hat keinen einzelnen Hex-Wert, gegen den sich rechnen ließe.
  $layer "$1" rp-mark "$tmp/l-mark.svg"
  $layer "$1" rp-plate "$tmp/l-plate.svg"
  # Gemessen wird bei 512 px, nicht bei der Anzeigegröße. Der Saum ist zwei
  # Pixel breit; bei kleiner Rendergröße sind das ein Prozent der Kantenlänge
  # und der Saum greift bis in die Gesichtsscheibe hinein. Dann entscheidet
  # die Auflösung der Messung über das Urteil statt die Zeichnung.
  rsvg-convert -w 512 -h 512 "$tmp/l-mark.svg" -o "$tmp/l-mark.png"
  rsvg-convert -w 512 -h 512 "$tmp/l-plate.svg" -o "$tmp/l-plate.png"
  report_contrast $($measure pair-edge-contrast "$tmp/l-mark.png" "$tmp/l-plate.png") "$2"
}

check_v5() {   # <svg> <label> <budget>
  read -r shapes maxcmd < <($measure shape-stats "$1")
  local budget=$3
  [ "$shapes" -le "$budget" ] \
    && ok "V5 Formzahl $2: $shapes ≤ $budget" \
    || bad "V5 Formzahl $2: $shapes > $budget"
  [ "$maxcmd" -le 400 ] \
    && ok "V5 größter Pfad $2: $maxcmd Befehle" \
    || bad "V5 größter Pfad $2: $maxcmd Befehle > 400 — sieht nach Trace aus"
}

check_v7() {   # <svg>
  # Formen werden gezählt, nicht Zeilen gegrept. `grep -c '<path'` zählt
  # Zeilen mit mindestens einem Treffer: zwei Pfade in einer Zeile ergaben
  # „genau ein Pfad", und jeder optimierte Export löst genau das aus.
  read -r shapes _ < <($measure shape-stats "$1")
  [ "$shapes" -eq 1 ] && ok "V7 genau eine Form" || bad "V7 $shapes Formen, erwartet genau 1"
  grep -q 'viewBox="0 0 16 16"' "$1" && ok "V7 viewBox" || bad "V7 viewBox ist nicht 0 0 16 16"
  # Konturen und Verläufe auch in `style="…"`-Schreibweise verbieten: das ist
  # Inkscapes Standard-Exportform, und Inkscape steht auf diesem Rechner.
  for forbidden in 'transform=' 'stroke=' 'stroke *:' 'linearGradient' 'radialGradient'; do
    grep -Eq "$forbidden" "$1" \
      && bad "V7 enthält $forbidden" \
      || ok "V7 ohne $forbidden"
  done
}

# V8: Androids Themed Icon zeigt die Silhouette, der Launcher sonst die
# farbige Fassung. Weichen beide voneinander ab, zeigt derselbe Launcher je
# nach Einstellung zwei verschiedene Eulen.
check_v8() {   # <farbig.svg> <silhouette.svg>
  rsvg-convert -w 256 -a "$1" -o "$tmp/v8-a.png"
  rsvg-convert -w 256 -a "$2" -o "$tmp/v8-b.png"
  # Verglichen werden die **Umrisse**, Aussparungen aufgefüllt. Der rohe
  # Flächenvergleich bestraft sonst genau das, was an der Silhouette Absicht
  # ist: Augen und Schnabel sind dort Löcher und in der farbigen Fassung
  # Flächen.
  local j; j=$($measure outline-overlap "$tmp/v8-a.png" "$tmp/v8-b.png")
  awk "BEGIN{exit !($j >= 0.97)}" \
    && ok "V8 Deckung der Umrisse farbig/Silhouette: $j" \
    || bad "V8 Deckung der Umrisse farbig/Silhouette: $j < 0.97 — zwei verschiedene Formen"
}

# V9: Androids garantierte Fläche ist der 66-dp-Kreis, nicht das
# 72-dp-Quadrat. Nur er ist auf jeder Maskenform sichtbar.
check_v9() {   # <vectordrawable.xml>
  local r; r=$(python3 - "$1" <<'PY'
import re, subprocess, sys, tempfile, pathlib
xml = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
paths = re.findall(r'android:pathData="([^"]+)"', xml)
body = "".join(f'<path fill="#000" d="{d}"/>' for d in paths)
svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 108 108" '
       f'width="108" height="108">{body}</svg>')
with tempfile.TemporaryDirectory() as tmp:
    source = pathlib.Path(tmp) / "vd.svg"
    png = pathlib.Path(tmp) / "vd.png"
    source.write_text(svg, encoding="utf-8")
    subprocess.run(["rsvg-convert", "-w", "432", "-h", "432", str(source),
                    "-o", str(png)], check=True, capture_output=True)
    sys.path.insert(0, "scripts/lib")
    from logo_measure import radius
    print(f"{radius(png) * 108:.2f}")
PY
)
  awk "BEGIN{exit !($r <= 33.5)}" \
    && ok "V9 Radius ${r} dp ≤ 33 dp (66-dp-Kreis)" \
    || bad "V9 Radius ${r} dp > 33 dp — wird auf Kreismasken beschnitten"
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

  # V3 muss auch Inkscapes Schreibweise abflachen. Ein `sed` auf
  # `fill="…"` ließ `style="fill:…"` unberührt und maß danach die
  # unveränderte Farbzeichnung — ein Test, der nichts prüft.
  cat > "$tmp/styled.svg" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><path style="fill:#ff0000;opacity:0.4" d="M0 0h64v64H0z"/></svg>
EOF
  $flatten "$tmp/styled.svg" "$tmp/styled-flat.svg"
  grep -q 'fill:#000000' "$tmp/styled-flat.svg" && grep -q 'opacity:1' "$tmp/styled-flat.svg" \
    && ok "Selbsttest V3 fasst style=\"fill:…\" an" \
    || bad "Selbsttest V3 lässt style=\"fill:…\" stehen"

  # V4 muss einen Verlauf erkennen, gegen den die Marke versinkt.
  awk "BEGIN{exit !($($measure contrast 2B155E 1B082D) < 1.5)}" \
    && ok "Selbsttest V4 erkennt 1,31:1 als blind" \
    || bad "Selbsttest V4 hält 1,31:1 für sichtbar"
}

check_mark() {   # <svg>
  echo "Marke: $1"
  check_v5 "$1" "Marke" "$MARK_SHAPES"
  # V1 fragt nach der Geometrie — füllt die Zeichnung ihr Raster aus? Bei
  # 16 px entscheidet darüber die Kantenglättung: eine Federspitze, die eine
  # halbe Rasterzeile hoch ist, landet unter der Alphaschwelle und fehlt in
  # der Messung. Gemessen wird deshalb groß.
  rsvg-convert -w 128 -a "$1" -o "$tmp/s.png"
  check_v1 "$tmp/s.png" "Marke" "$1"
}

check_silhouette() {   # <svg>
  echo "Einfarbige Fassung bei ${MARK_SIZE}px: $1"
  # Hier greift der Negativraum-Test: die Augen sind Ringe und der Schnabel
  # ein Loch, und genau die laufen bei kleiner Größe als erstes zu. An der
  # farbigen Fassung wäre dieselbe Frage sinnlos — dort sind es Flächen.
  rsvg-convert -w "$MARK_SIZE" -a "$1" -o "$tmp/sil.png"
  check_v2 "$tmp/sil.png" "${MARK_SIZE}px"
  check_v3 "$1" "Silhouette" "$MARK_SIZE"
}

check_all() {
  local icons=data/icons/hicolor brand=data/brand
  local android=android/app/src/main/res

  echo "Kalibrierung der Detektoren"
  self_test

  check_mark "$brand/mark.svg"
  check_silhouette "$brand/mark-mono.svg"

  echo "Symbolic: $icons/symbolic/apps/org.reprise.Reprise-symbolic.svg"
  check_v7 "$icons/symbolic/apps/org.reprise.Reprise-symbolic.svg"
  rsvg-convert -w 16 -h 16 "$icons/symbolic/apps/org.reprise.Reprise-symbolic.svg" \
    -o "$tmp/sym.png"
  check_v2 "$tmp/sym.png" "16px"
  # Kein Kontrasttest: GNOME färbt Symbolic-Icons zur Laufzeit mit der
  # Vordergrundfarbe des Themes um. Der literale Wert #222222 wird nie
  # angezeigt. Was hier zählt, ist die Silhouette — und die prüft V2.

  echo "App-Icon auf der Platte"
  check_v4_plate "$icons/scalable/apps/org.reprise.Reprise.svg" "Marke auf der Platte"
  check_v4_plate "$brand/favicon.svg" "Marke auf der randlosen Platte"

  echo "Fassung für dunkle Gründe"
  rsvg-convert -w 256 -a "$brand/mark-on-dark.svg" -o "$tmp/on-dark.png"
  check_v4_ground "$tmp/on-dark.png" 1B082D "mark-on-dark auf #1B082D"
  rsvg-convert -w 256 -a "$brand/mark.svg" -o "$tmp/on-light.png"
  check_v4_ground "$tmp/on-light.png" FFFFFF "mark auf Weiß"

  echo "Android"
  check_v8 "$brand/mark.svg" "$brand/mark-mono.svg"
  check_v9 "$android/drawable/ic_launcher_foreground.xml"
  check_v9 "$android/drawable/ic_launcher_monochrome.xml"

  echo "Herkunft der abgeleiteten Dateien"
  ./scripts/build-brand-assets.sh --check
}

case ${1:-} in
  --self-test) echo "Kalibrierung der Detektoren"; self_test ;;
  --all)       check_all ;;
  --symbolic)
    svg=$2; echo "Symbolic: $svg"
    check_v7 "$svg"
    rsvg-convert -w 16 -h 16 "$svg" -o "$tmp/sym.png"
    check_v2 "$tmp/sym.png" "16px" ;;
  --mark)      check_mark "$2" ;;
  --silhouette) check_silhouette "$2" ;;
  *)
    echo "Aufruf: $0 --all | --mark <svg> | --silhouette <svg> | --symbolic <svg> | --self-test" >&2
    exit 2 ;;
esac

exit $fail
