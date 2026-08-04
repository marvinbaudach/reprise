#!/usr/bin/env bash
# Erzeugt jede abgeleitete Markendatei aus den Zeichnungen unter data/brand.
#
# Handgezeichnet und damit Quelle sind nur:
#   data/brand/mark.svg              volle Stufe, ab 128 px
#   data/brand/mark-reduced.svg      vereinfacht, 48–64 px und Android
#   data/brand/mark-micro.svg        ein Pfad, ≤ 32 px
#   data/brand/mark-mono.svg         Silhouette der vollen Stufe
#   data/brand/mark-reduced-mono.svg Silhouette der vereinfachten Stufe
#   data/brand/icon-plate.svg        Grundfläche und Verlauf des App-Icons
#
# Alles andere entsteht hier. Das ist der Grund: das App-Icon war früher
# eine Kopie der Marke, und die kleinen Stufen bekamen die Platte nie —
# zwei verschiedene Icons für dieselbe App. Was erzeugt wird, kann nicht
# auseinanderlaufen.
#
#   ./scripts/build-brand-assets.sh            schreibt in den Baum
#   ./scripts/build-brand-assets.sh --check    erzeugt daneben und vergleicht
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

lib=scripts/lib
brand=data/brand
icons=data/icons/hicolor
android=android/app/src/main/res
font=$brand/fonts/Fraunces-SemiBold.ttf

mode=${1:-write}
tmp=$(mktemp -d)
out=$root
[ "$mode" = "--check" ] && out=$(mktemp -d)
cleanup() { rm -rf "$tmp"; [ "$out" != "$root" ] && rm -rf "$out"; return 0; }
trap cleanup EXIT

# Zielpfad im Ausgabebaum. Im Prüfmodus liegt daneben, was sonst überschriebe.
dest() {
  if [ "$out" = "$root" ]; then printf '%s\n' "$1"; else
    mkdir -p "$out/$(dirname "$1")"; printf '%s\n' "$out/$1"
  fi
}

say() { printf '  %s\n' "$*"; }

# Kantenlänge des Kastens, in den die Zeichnung auf der 128er Platte passt.
# Die volle Stufe ist breiter als hoch und darf deshalb die Breite fast
# ausschöpfen; die kleinen Stufen sind fast quadratisch und brauchen Rand,
# sonst stoßen sie an die gerundete Ecke der Platte.
box_full=94
box_reduced=82
box_micro=78

echo "== App-Icon: Platte + Zeichnung =="
python3 $lib/compose_icon.py $brand/icon-plate.svg $brand/mark.svg \
  "$(dest $icons/scalable/apps/org.reprise.Reprise.svg)" \
  --box-width $box_full --box-height $box_full
say "scalable ← mark.svg"

# Die kleinen Stufen bekommen eine eigene Komposition. Sie liegt als Datei
# im Baum und nicht nur im Temporärverzeichnis: sonst müsste das Gate die
# Kastengröße ein zweites Mal kennen, um sie nachzubauen — und zwei Stellen,
# die dieselbe Zahl kennen, laufen auseinander.
python3 $lib/compose_icon.py $brand/icon-plate.svg $brand/mark-reduced.svg \
  "$(dest $brand/icon-reduced.svg)" \
  --box-width $box_reduced --box-height $box_reduced
say "icon-reduced.svg ← mark-reduced.svg"

for size in 512 256 128; do
  mkdir -p "$(dirname "$(dest $icons/${size}x${size}/apps/org.reprise.Reprise.png)")"
  rsvg-convert -w $size -h $size "$(dest $icons/scalable/apps/org.reprise.Reprise.svg)" \
    -o "$(dest $icons/${size}x${size}/apps/org.reprise.Reprise.png)"
  say "${size}px ← scalable"
done
for size in 64 48; do
  mkdir -p "$(dirname "$(dest $icons/${size}x${size}/apps/org.reprise.Reprise.png)")"
  rsvg-convert -w $size -h $size "$(dest $brand/icon-reduced.svg)" \
    -o "$(dest $icons/${size}x${size}/apps/org.reprise.Reprise.png)"
  say "${size}px ← mark-reduced.svg"
done

# GNOME färbt Symbolic-Icons zur Laufzeit um; #222222 ist die Konvention.
# Erzeugt statt gezeichnet, weil die Micro-Stufe bereits genau das ist, was
# das Symbolic sein muss: ein Pfad auf 16er Raster, ohne Verlauf und ohne
# Kontur. Zwei Hände an derselben Silhouette hieße, sie auseinanderlaufen
# zu lassen.
python3 $lib/svg_recolour.py $brand/mark-micro.svg \
  "$(dest $icons/symbolic/apps/org.reprise.Reprise-symbolic.svg)" '#1F1056=#222222'
say "symbolic ← mark-micro.svg"

echo "== Android: adaptives Icon =="
# Der Vordergrund kommt aus der vereinfachten Stufe. Sie ist flach gefüllt;
# VectorDrawable kennt keine Verlaufsverweise, und die volle Stufe lebt von
# ihren Verläufen. Auf einem Launcher wird das Icon ohnehin klein gezeigt.
python3 $lib/svg_to_vectordrawable.py $brand/mark-reduced.svg \
  "$(dest $android/drawable/ic_launcher_foreground.xml)"
say "foreground ← mark-reduced.svg"
python3 $lib/svg_to_vectordrawable.py $brand/mark-reduced-mono.svg \
  "$(dest $android/drawable/ic_launcher_monochrome.xml)" --mono
say "monochrome ← mark-reduced-mono.svg"
python3 $lib/plate_to_vectordrawable.py $brand/icon-plate.svg \
  "$(dest $android/drawable/ic_launcher_background.xml)"
say "background ← icon-plate.svg"

echo "== Web =="
# Randlos für Apple und den Play Store: beide maskieren selbst, eine eigene
# Rundung darunter erzeugte nur einen sichtbaren Saum.
python3 $lib/compose_icon.py $brand/icon-plate.svg $brand/mark.svg \
  "$tmp/icon-bleed.svg" --box-width $box_full --box-height $box_full \
  --plate-inset 0 --plate-radius 0
rsvg-convert -w 180 -h 180 "$tmp/icon-bleed.svg" \
  -o "$(dest $brand/apple-touch-icon-180.png)"
say "apple-touch-icon-180.png"
# Der Play Store nimmt keine Transparenz an; die randlose Platte deckt.
rsvg-convert -w 512 -h 512 "$tmp/icon-bleed.svg" \
  -o "$(dest $brand/play-store-icon-512.png)"
say "play-store-icon-512.png"

python3 $lib/compose_icon.py $brand/icon-plate.svg $brand/mark-micro.svg \
  "$(dest $brand/favicon.svg)" --box-width $box_micro --box-height $box_micro \
  --plate-inset 0 --plate-radius 22
rsvg-convert -w 32 -h 32 "$(dest $brand/favicon.svg)" \
  -o "$(dest $brand/favicon-32.png)"
say "favicon.svg + favicon-32.png ← mark-micro.svg"

echo "== Fassung für dunkle Gründe =="
# Dieselbe Zeichnung mit angehobenen Körperwerten. Als zweite Datei gepflegt
# lief sie auseinander; erzeugt kann sie es nicht.
python3 $lib/svg_recolour.py $brand/mark.svg "$(dest $brand/mark-on-dark.svg)" \
  --prefix rp-od- \
  '#1F1056=#7A56B0' '#2B155E=#8262BC' '#33195F=#8A67C0' '#3A2470=#9478CC' \
  '#76388F=#C09AD0' '#9E88AB=#D8C6E2'
say "mark-on-dark.svg ← mark.svg"

echo "== Lockups =="
for mode_name in horizontal vertical; do
  python3 $lib/compose_lockup.py $brand/mark.svg $font \
    --mode $mode_name --size 268 --tracking 2 --mark-height 320 \
    --prefix "rp-l$(printf '%.1s' "$mode_name")-" \
    --live "$(dest $brand/lockup-$mode_name.svg)" \
    --outlined "$(dest $brand/lockup-$mode_name-outlined.svg)"
  say "lockup-$mode_name.svg + -outlined.svg"
done

if [ "$mode" = "--check" ]; then
  echo "== Abgleich mit dem Baum =="
  status=0
  while IFS= read -r generated; do
    relative=${generated#"$out"/}
    if ! cmp -s "$generated" "$relative"; then
      printf '  FAIL  %s weicht von den Zeichnungen ab\n' "$relative" >&2
      status=1
    fi
  done < <(find "$out" -type f)
  [ $status -eq 0 ] && echo "  ok    jede abgeleitete Datei stammt aus den Zeichnungen"
  exit $status
fi

echo "Fertig. Prüfen mit: ./scripts/check-logo-artwork.sh --all"
