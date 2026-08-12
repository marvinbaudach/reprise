#!/usr/bin/env bash
# Build every derived brand file from palette.toml and the exact geometry.
#
# The palette and generator are maintained; SVG sources, platform resources,
# raster stages, web assets and lockups are reproducible output.
#
#   ./scripts/build-brand-assets.sh
#   ./scripts/build-brand-assets.sh --check
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

lib=scripts/lib
palette=data/brand/palette.toml
font=data/brand/fonts/Fraunces-SemiBold.ttf
read -r palette_teal palette_coral < <(python3 - "$palette" <<'PY'
import sys
import tomllib
with open(sys.argv[1], "rb") as source:
    palette = tomllib.load(source)
print(palette["reprise_teal"], palette["reprise_coral"])
PY
)
mode=${1:-write}
if [[ $mode != write && $mode != --check ]]; then
  printf 'usage: %s [--check]\n' "$0" >&2
  exit 2
fi

scratch=$(mktemp -d)
out=$root
[[ $mode == --check ]] && out=$(mktemp -d)
cleanup() {
  rm -rf "$scratch"
  [[ $out == "$root" ]] || rm -rf "$out"
}
trap cleanup EXIT

target() {
  local relative=$1
  mkdir -p "$out/$(dirname "$relative")"
  printf '%s\n' "$out/$relative"
}

say() { printf '  %s\n' "$*"; }

echo "== Palette-backed source drawings =="
generated_brand=$(target data/brand/.keep)
generated_brand=$(dirname "$generated_brand")
rm -f "$generated_brand/.keep"
python3 "$lib/brand_sources.py" "$palette" "$generated_brand"
say "four 96-unit sources plus the 16-unit stage ← palette.toml"

mark=$generated_brand/reprise-mark.svg
mark_light=$generated_brand/reprise-mark-light.svg
mark_mono=$generated_brand/reprise-mark-mono.svg
plate=$generated_brand/icon-plate.svg
icon_16=$generated_brand/reprise-icon-16.svg
mark_16_mono=$generated_brand/reprise-mark-16-mono.svg
first_aid_symbolic=$generated_brand/reprise-first-aid-symbolic.svg

build_surface_tree() {
  local tree_root=$out
  local brand=$tree_root/data/brand
  local icons=$tree_root/data/icons/hicolor
  local android=$tree_root/android/app/src/main/res
  local icon bleed
  mkdir -p "$brand"

  icon=$brand/reprise-icon.svg
  python3 "$lib/compose_icon.py" "$plate" "$mark" "$icon" --native

  mkdir -p "$icons/scalable/apps"
  cp "$icon" "$icons/scalable/apps/io.github.marvinbaudach.Reprise.svg"
  # Every stage from 22px up comes from the 96-unit drawing. 16px comes from
  # its own grid-aligned source: rasterising the 96-unit mark that small turns
  # the dots into two specks below the noise floor, and no amount of scaling
  # fixes a feature that is under a pixel.
  for size in 22 24 32 48 64 128 256 512; do
    mkdir -p "$icons/${size}x${size}/apps"
    rsvg-convert -w "$size" -h "$size" "$icon" \
      -o "$icons/${size}x${size}/apps/io.github.marvinbaudach.Reprise.png"
  done
  mkdir -p "$icons/16x16/apps"
  rsvg-convert -w 16 -h 16 "$icon_16" \
    -o "$icons/16x16/apps/io.github.marvinbaudach.Reprise.png"
  mkdir -p "$icons/symbolic/apps"
  python3 "$lib/svg_recolour.py" "$mark_mono" \
    "$icons/symbolic/apps/io.github.marvinbaudach.Reprise-symbolic.svg" \
    'currentColor=#222222'
  cp "$first_aid_symbolic" \
    "$icons/symbolic/apps/reprise-first-aid-symbolic.svg"

  mkdir -p "$android/drawable"
  python3 "$lib/svg_to_vectordrawable.py" "$mark" \
    "$android/drawable/ic_launcher_foreground.xml" \
    --fixed-offset 6 \
    --colour-map "$palette_teal=@color/reprise_teal" \
    --colour-map "$palette_coral=@color/reprise_coral"
  python3 "$lib/svg_to_vectordrawable.py" "$mark_mono" \
    "$android/drawable/ic_launcher_monochrome.xml" \
    --fixed-offset 6 --mono
  python3 "$lib/svg_to_vectordrawable.py" "$mark_mono" \
    "$android/drawable/ic_repeat_sign.xml" \
    --fixed-offset 6 --mono --mono-fill '@android:color/white' \
    --tint '?android:attr/colorControlNormal'
  python3 "$lib/plate_to_vectordrawable.py" "$plate" \
    "$android/drawable/ic_launcher_background.xml" \
    --colour-ref '@color/reprise_plate'
  python3 "$lib/android_icon_resources.py" "$palette" "$android"

  bleed=$scratch/bleed.svg
  python3 "$lib/compose_icon.py" "$plate" "$mark" "$bleed" \
    --native --plate-inset 0 --plate-radius 0
  python3 "$lib/legacy_launcher.py" "$bleed" "$android"

  python3 "$lib/compose_icon.py" "$plate" "$mark" \
    "$brand/favicon.svg" --native --plate-inset 0 --plate-radius 22
  rsvg-convert -w 32 -h 32 "$brand/favicon.svg" -o "$brand/favicon-32.png"
  rsvg-convert -w 180 -h 180 "$bleed" -o "$brand/apple-touch-icon-180.png"
  rsvg-convert -w 512 -h 512 "$bleed" -o "$brand/play-store-icon-512.png"

  for lockup_mode in horizontal vertical; do
    python3 "$lib/compose_lockup.py" "$mark" "$font" \
      --mode "$lockup_mode" --size 268 --tracking 2 --mark-height 320 \
      --prefix "rp-l$(printf '%.1s' "$lockup_mode")-" \
      --live "$brand/lockup-$lockup_mode.svg" \
      --outlined "$brand/lockup-$lockup_mode-outlined.svg"
  done

  # Two sources ship without anything here consuming them: the light mark for
  # pale surfaces this repository does not build, and the single-colour 16-unit
  # form for a themed 16px surface that would otherwise be redrawn by hand.
  # Assert them rather than let a rename silently drop them.
  test -f "$mark_light"
  test -f "$mark_16_mono"
}

echo "== Asset tree =="
build_surface_tree
say "desktop, Android, web and lockups"

if [[ $mode == --check ]]; then
  echo "== Compare generated files with the worktree =="
  result=0
  while IFS= read -r generated; do
    relative=${generated#"$out"/}
    if ! cmp -s "$generated" "$relative"; then
      printf '  FAIL  %s differs from generated output\n' "$relative" >&2
      result=1
    fi
  done < <(find "$out" -type f -print | sort)
  if [[ $result -eq 0 ]]; then
    echo "  ok    every derived file comes from palette and geometry"
  fi
  exit "$result"
fi

echo "Done. Verify with: ./scripts/check-logo-artwork.sh --all"
