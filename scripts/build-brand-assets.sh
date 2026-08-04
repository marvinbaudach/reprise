#!/usr/bin/env bash
# Build every derived brand file from palette.toml and the exact geometry.
#
# The palette and generator are maintained; SVG sources, platform resources,
# raster stages, web assets and comparison trees are reproducible output.
#
#   ./scripts/build-brand-assets.sh
#   ./scripts/build-brand-assets.sh --check
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

lib=scripts/lib
palette=data/brand/palette.toml
font=data/brand/fonts/Fraunces-SemiBold.ttf
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
say "six 96-unit SVG sources ← palette.toml"

mark_a=$generated_brand/reprise-mark-a.svg
mark_b=$generated_brand/reprise-mark-b.svg
mark_a_light=$generated_brand/reprise-mark-a-light.svg
mark_b_light=$generated_brand/reprise-mark-b-light.svg
mark_mono=$generated_brand/reprise-mark-mono.svg
plate=$generated_brand/icon-plate.svg

build_surface_tree() { # <tree root relative to output> <active variant>
  local tree=$1 active=$2
  local tree_root=$out/$tree
  local brand=$tree_root/data/brand
  local icons=$tree_root/data/icons/hicolor
  local android=$tree_root/android/app/src/main/res
  local active_mark active_light active_icon slug bleed
  mkdir -p "$brand"

  python3 "$lib/compose_icon.py" "$plate" "$mark_a" \
    "$brand/reprise-icon-a.svg" --native
  python3 "$lib/compose_icon.py" "$plate" "$mark_b" \
    "$brand/reprise-icon-b.svg" --native
  active_mark=$mark_a
  active_light=$mark_a_light
  active_icon=$brand/reprise-icon-a.svg
  if [[ $active == b ]]; then
    active_mark=$mark_b
    active_light=$mark_b_light
    active_icon=$brand/reprise-icon-b.svg
  fi

  mkdir -p "$icons/scalable/apps"
  cp "$active_icon" "$icons/scalable/apps/org.reprise.Reprise.svg"
  for size in 16 22 24 32 48 64 128 256 512; do
    mkdir -p "$icons/${size}x${size}/apps"
    rsvg-convert -w "$size" -h "$size" "$active_icon" \
      -o "$icons/${size}x${size}/apps/org.reprise.Reprise.png"
  done
  mkdir -p "$icons/symbolic/apps"
  python3 "$lib/svg_recolour.py" "$mark_mono" \
    "$icons/symbolic/apps/org.reprise.Reprise-symbolic.svg" \
    'currentColor=#222222'

  mkdir -p "$android/drawable"
  for variant in a b; do
    local variant_mark=$mark_a
    [[ $variant == b ]] && variant_mark=$mark_b
    python3 "$lib/svg_to_vectordrawable.py" "$variant_mark" \
      "$android/drawable/ic_launcher_foreground_${variant}.xml" \
      --fixed-offset 6 \
      --colour-map '#4FDBD4=@color/reprise_teal' \
      --colour-map '#A855F7=@color/reprise_violet'
  done
  python3 "$lib/svg_to_vectordrawable.py" "$mark_mono" \
    "$android/drawable/ic_launcher_monochrome.xml" \
    --fixed-offset 6 --mono
  python3 "$lib/svg_to_vectordrawable.py" "$mark_mono" \
    "$android/drawable/ic_repeat_sign.xml" \
    --fixed-offset 6 --mono --mono-fill '@android:color/white' \
    --tint '?attr/colorControlNormal'
  python3 "$lib/plate_to_vectordrawable.py" "$plate" \
    "$android/drawable/ic_launcher_background.xml" \
    --colour-ref '@color/reprise_plate'
  python3 "$lib/android_icon_resources.py" "$palette" "$android" --active "$active"

  slug=$(printf '%s-%s' "$tree" "$active" | tr '/.' '__')
  bleed=$scratch/${slug}-bleed.svg
  python3 "$lib/compose_icon.py" "$plate" "$active_mark" "$bleed" \
    --native --plate-inset 0 --plate-radius 0
  python3 "$lib/legacy_launcher.py" "$bleed" "$android"

  python3 "$lib/compose_icon.py" "$plate" "$active_mark" \
    "$brand/favicon.svg" --native --plate-inset 0 --plate-radius 22
  rsvg-convert -w 32 -h 32 "$brand/favicon.svg" -o "$brand/favicon-32.png"
  rsvg-convert -w 180 -h 180 "$bleed" -o "$brand/apple-touch-icon-180.png"
  rsvg-convert -w 512 -h 512 "$bleed" -o "$brand/play-store-icon-512.png"

  for lockup_mode in horizontal vertical; do
    python3 "$lib/compose_lockup.py" "$active_mark" "$font" \
      --mode "$lockup_mode" --size 268 --tracking 2 --mark-height 320 \
      --prefix "rp-l$(printf '%.1s' "$lockup_mode")-" \
      --live "$brand/lockup-$lockup_mode.svg" \
      --outlined "$brand/lockup-$lockup_mode-outlined.svg"
  done

  # The light mark is intentionally consumed only by the comparison sheet.
  # Keeping the variable here makes that platform distinction explicit.
  test -f "$active_light"
}

echo "== Active asset tree: variant A =="
build_surface_tree . a
say "desktop, Android, web and lockups"

echo "== Device-comparison trees =="
build_surface_tree data/brand/variants/a a
build_surface_tree data/brand/variants/b b
say "variants/a and variants/b mirror the shipped subtree layout"

echo "== Self-contained comparison sheet =="
python3 "$lib/compare_sheet.py" "$(target data/brand/variants/compare.html)" \
  --a "$mark_a" "$mark_a_light" "$generated_brand/reprise-icon-a.svg" \
  --b "$mark_b" "$mark_b_light" "$generated_brand/reprise-icon-b.svg"
say "data/brand/variants/compare.html"

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
