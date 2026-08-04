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
