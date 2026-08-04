#!/usr/bin/env bash
# Measure the repeat-sign artwork against its geometry and delivery contract.
#
# The hard small-size gate is 28 px. The 16/22/24/32 px stages are reported
# separately because 16 px is a physical raster limit, not licence to distort
# the specified 96-unit geometry.
set -euo pipefail

repo_root=${LOGO_ARTWORK_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
cd "$repo_root"

measure=(python3 scripts/lib/logo_measure.py)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

fail=0
ok() { printf '  ok    %s\n' "$*"; }
bad() { printf '  FAIL  %s\n' "$*" >&2; fail=1; }

MARK_SIZE=28
MARK_PARTS=4
MIN_CONTRAST=3.0
MAX_CIRCLE_CLIP=0.01
GEOMETRY_TOLERANCE=$(awk 'BEGIN { print 96 / 512 }')

read -r TEAL VIOLET TEAL_LIGHT VIOLET_LIGHT PLATE < <(python3 - <<'PY'
import tomllib
with open("data/brand/palette.toml", "rb") as source:
    palette = tomllib.load(source)
print(*(palette[key] for key in (
    "reprise_teal", "reprise_violet", "reprise_teal_light",
    "reprise_violet_light", "reprise_plate")))
PY
)

check_source_contract() {
  if python3 - <<'PY'
import re
import tomllib
from pathlib import Path
from xml.etree import ElementTree as ET

brand = Path("data/brand")
with (brand / "palette.toml").open("rb") as source:
    palette = tomllib.load(source)
ns = {"svg": "http://www.w3.org/2000/svg"}
geometry = (
    ("circle", {"cx": "30", "cy": "39", "r": "5.5"}),
    ("circle", {"cx": "30", "cy": "57", "r": "5.5"}),
    ("rect", {"x": "41", "y": "20", "width": "5", "height": "56", "rx": "1"}),
    ("rect", {"x": "52", "y": "20", "width": "15", "height": "56", "rx": "1.5"}),
)
colours = {
    "reprise-mark-a.svg": (palette["reprise_violet"], palette["reprise_teal"]),
    "reprise-mark-b.svg": (palette["reprise_teal"], palette["reprise_violet"]),
    "reprise-mark-a-light.svg": (palette["reprise_violet_light"], palette["reprise_teal_light"]),
    "reprise-mark-b-light.svg": (palette["reprise_teal_light"], palette["reprise_violet_light"]),
}
for name, (small, large) in colours.items():
    root = ET.parse(brand / name).getroot()
    assert root.attrib["viewBox"] == "0 0 96 96", name
    shapes = list(root)
    assert len(shapes) == 4, name
    for index, (node, (tag, attributes)) in enumerate(zip(shapes, geometry, strict=True)):
        assert node.tag.rsplit("}", 1)[-1] == tag, (name, index)
        assert all(node.attrib.get(key) == value for key, value in attributes.items()), (name, index)
        assert node.attrib["fill"] == (small if index < 3 else large), (name, index)
    assert not re.search(r"(?:linear|radial)Gradient", ET.tostring(root, encoding="unicode"))
mono = ET.parse(brand / "reprise-mark-mono.svg").getroot()
assert mono.attrib["viewBox"] == "0 0 96 96"
assert mono.attrib["fill"] == "currentColor"
assert len(list(mono)) == 4
plate = ET.parse(brand / "icon-plate.svg").getroot()
assert plate.attrib["viewBox"] == "0 0 96 96"
group = plate.find("svg:g", ns)
rects = group.findall("svg:rect", ns)
assert group.attrib["id"] == "rp-plate" and len(rects) == 1
assert rects[0].attrib == {
    "x": "4", "y": "4", "width": "88", "height": "88",
    "rx": "22", "fill": palette["reprise_plate"],
}
PY
  then
    ok "source geometry: ordered circles, 1:3 barlines and solid 96-unit plate"
  else
    bad "source geometry differs from the specified 96-unit drawing"
  fi
}

check_geometry_box() { # <variant>
  local variant=$1 png=$tmp/geometry-$1.png
  rsvg-convert -w 512 -h 512 -a "data/brand/reprise-mark-$variant.svg" -o "$png"
  local sx0 sy0 sx1 sy1 x0 y0 x1 y1
  read -r sx0 sy0 sx1 sy1 < <("${measure[@]}" ink-box "$png")
  read -r x0 y0 x1 y1 < <(awk -v a="$sx0" -v b="$sy0" -v c="$sx1" -v d="$sy1" \
    'BEGIN { printf "%.3f %.3f %.3f %.3f\n", a*96, b*96, c*96, d*96 }')
  if awk -v x0="$x0" -v y0="$y0" -v x1="$x1" -v y1="$y1" \
      -v t="$GEOMETRY_TOLERANCE" 'BEGIN {
        ok=(x0 >= 24.5-t && x0 <= 24.5+t && y0 >= 20-t && y0 <= 20+t &&
            x1 >= 67-t && x1 <= 67+t && y1 >= 76-t && y1 <= 76+t); exit !ok
      }'; then
    ok "V1 variant $variant ink box at 512px: x=[$x0,$x1], y=[$y0,$y1] viewBox units (±1px)"
  else
    bad "V1 variant $variant ink box x=[$x0,$x1], y=[$y0,$y1] misses the exact geometry"
  fi
}

report_components() { # <variant>
  local variant=$1 size png count
  for size in 16 22 24 28 32; do
    png=$tmp/$variant-$size.png
    rsvg-convert -w "$size" -h "$size" -a \
      "data/brand/reprise-mark-$variant.svg" -o "$png"
    count=$("${measure[@]}" ink-components "$png")
    if [[ $size -eq $MARK_SIZE ]]; then
      [[ $count -eq $MARK_PARTS ]] \
        && ok "V2 variant $variant ${size}px gate: $count separate components" \
        || bad "V2 variant $variant ${size}px gate: $count instead of $MARK_PARTS components"
    elif [[ $count -lt $MARK_PARTS ]]; then
      # Say what was measured, not what it probably means. A stage can fall
      # short two ways and they call for opposite fixes: strokes running into
      # each other (geometry too tight) or strokes surviving but too small to
      # count (the sign is simply below its useful size). The raw group sizes
      # tell them apart — groups still equal to MARK_PARTS means nothing
      # merged.
      local sizes groups floor
      sizes=$("${measure[@]}" ink-component-sizes "$png")
      floor=$("${measure[@]}" noise-floor "$png")
      groups=$(wc -w <<<"$sizes")
      if [[ $groups -eq $MARK_PARTS ]]; then
        ok "V2 variant $variant ${size}px report: $count of $MARK_PARTS components clear the ${floor}px noise floor; all $MARK_PARTS pixel groups survive separately at sizes [$sizes] — nothing merged, the small elements are just under the floor"
      else
        ok "V2 variant $variant ${size}px report: $count components above the ${floor}px noise floor; $groups pixel groups at sizes [$sizes] — strokes have run together"
      fi
    else
      ok "V2 variant $variant ${size}px report: $count separate components"
    fi
  done
}

check_pair_contrast() { # <variant> <small> <large> <ground> <label>
  local variant=$1 small=$2 large=$3 ground=$4 label=$5 a b
  a=$("${measure[@]}" contrast "$small" "$ground")
  b=$("${measure[@]}" contrast "$large" "$ground")
  if awk -v a="$a" -v b="$b" -v floor="$MIN_CONTRAST" \
      'BEGIN { exit !(a >= floor && b >= floor) }'; then
    ok "V4 variant $variant on $label: small $a:1, thick $b:1"
  else
    bad "V4 variant $variant on $label: small $a:1, thick $b:1 (minimum $MIN_CONTRAST:1)"
  fi
}

check_v7() {
  local symbolic=data/icons/hicolor/symbolic/apps/org.reprise.Reprise-symbolic.svg
  local shapes
  shapes=$("${measure[@]}" shape-stats "$symbolic" | awk '{print $1}')
  [[ $shapes -eq 4 ]] && ok "V7 symbolic has the four specified shapes" \
    || bad "V7 symbolic has $shapes shapes instead of 4"
  grep -q 'viewBox="0 0 96 96"' "$symbolic" \
    && ok "V7 symbolic viewBox is 0 0 96 96" \
    || bad "V7 symbolic viewBox is not 0 0 96 96"
  local forbidden
  for forbidden in 'transform=' 'stroke=' 'stroke *:' 'linearGradient' 'radialGradient'; do
    if grep -Eq "$forbidden" "$symbolic"; then
      bad "V7 symbolic contains forbidden $forbidden"
    else
      ok "V7 symbolic has no $forbidden"
    fi
  done
  for source in data/brand/reprise-mark-{a,b,a-light,b-light}.svg; do
    if grep -Eq '(linear|radial)Gradient' "$source"; then
      bad "V7 coloured source contains a gradient: $source"
    else
      ok "V7 coloured source is gradient-free: $source"
    fi
  done
}

check_v8() { # <variant>
  local variant=$1 overlap
  rsvg-convert -w 256 -h 256 -a "data/brand/reprise-mark-$variant.svg" \
    -o "$tmp/v8-colour-$variant.png"
  rsvg-convert -w 256 -h 256 -a data/brand/reprise-mark-mono.svg \
    -o "$tmp/v8-mono-$variant.png"
  overlap=$("${measure[@]}" outline-overlap \
    "$tmp/v8-colour-$variant.png" "$tmp/v8-mono-$variant.png")
  if awk -v value="$overlap" 'BEGIN { exit !(value >= 0.99) }'; then
    ok "V8 variant $variant colour/mono outline overlap: $overlap"
  else
    bad "V8 variant $variant colour/mono outline overlap: $overlap < 0.99"
  fi
}

check_v9() { # <VectorDrawable> <label>
  local xml=$1 label=$2 svg=$tmp/v9-$2.svg png=$tmp/v9-$2.png
  python3 - "$xml" "$svg" <<'PY'
import pathlib
import re
import sys
xml = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
paths = re.findall(r'android:pathData="([^"]+)"', xml)
if len(paths) != 4:
    raise SystemExit(f"expected four VectorDrawable paths, found {len(paths)}")
body = "".join(f'<path fill="#000" d="{path}"/>' for path in paths)
pathlib.Path(sys.argv[2]).write_text(
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 108 108">'
    + body + '</svg>', encoding="utf-8")
PY
  rsvg-convert -w 1080 -h 1080 -a "$svg" -o "$png"
  local clipped before after
  read -r clipped before after < <("${measure[@]}" circle-clip "$png" "$(awk 'BEGIN { print 33/108 }')")
  if awk -v clipped="$clipped" -v limit="$MAX_CIRCLE_CLIP" \
      -v before="$before" -v after="$after" \
      'BEGIN { exit !(clipped <= limit && before == 4 && after == 4) }'; then
    ok "V9 $label under 66dp circle: clipped $clipped of ink; components $before→$after"
  else
    bad "V9 $label under 66dp circle: clipped $clipped (limit $MAX_CIRCLE_CLIP); components $before→$after"
  fi
}

check_palette_single_source() {
  if python3 - <<'PY'
import os
import tomllib
from pathlib import Path

root = Path(".")
palette_path = Path("data/brand/palette.toml")
with palette_path.open("rb") as source:
    palette = tomllib.load(source)
palette_text = palette_path.read_text(encoding="utf-8")
for value in palette.values():
    assert palette_text.count(value) == 1, value

skip_parts = {".git", "target", "build", ".gradle-user-home", ".cache-gradle", ".android-user"}
failures = []
for directory, names, files in os.walk(root):
    names[:] = [name for name in names if name not in skip_parts and not name.startswith(".cache-")]
    for name in files:
        path = Path(directory) / name
        relative = path.relative_to(root)
        if name.startswith(".pipeline-"):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except (UnicodeDecodeError, OSError):
            continue
        hits = [value for value in palette.values() if value in text]
        if not hits or relative == palette_path or relative.parts[0] == "docs":
            continue
        derived = (
            str(relative).startswith("data/icons/") or
            str(relative).startswith("data/brand/variants/") or
            (str(relative).startswith("data/brand/") and relative.suffix == ".svg" and
             ("Generated" in text or "Erzeugt" in text)) or
            (str(relative).startswith("android/app/src/main/res/") and "Generated" in text)
        )
        if not derived:
            failures.append(f"{relative}: {', '.join(hits)}")
if failures:
    raise SystemExit("maintained palette duplicates:\n" + "\n".join(failures))
PY
  then
    ok "palette literals have one maintained source: data/brand/palette.toml"
  else
    bad "a palette literal is maintained outside data/brand/palette.toml"
  fi
}

check_delivery() {
  local required size dimensions
  for size in 16 22 24 32 48 64 128 256 512; do
    required="data/icons/hicolor/${size}x${size}/apps/org.reprise.Reprise.png"
    [[ -f $required ]] || bad "missing hicolor stage: $required"
  done
  for required in \
    data/icons/hicolor/scalable/apps/org.reprise.Reprise.svg \
    data/icons/hicolor/symbolic/apps/org.reprise.Reprise-symbolic.svg \
    data/brand/reprise-icon-a.svg data/brand/reprise-icon-b.svg \
    data/brand/variants/compare.html \
    android/app/src/main/res/drawable/ic_repeat_sign.xml; do
    [[ -f $required ]] && ok "delivered: $required" || bad "missing: $required"
  done
  if grep -q 'transform=' data/icons/hicolor/scalable/apps/org.reprise.Reprise.svg; then
    bad "shipped scalable icon rescales the specified geometry"
  else
    ok "shipped scalable icon preserves the 96-unit coordinates without transform"
  fi
  if rg --pcre2 -q 'src="(?!data:image/png;base64,)' data/brand/variants/compare.html; then
    bad "comparison sheet has an external image resource"
  else
    ok "comparison sheet embeds every raster as a data URI"
  fi
}

self_test() {
  cat > "$tmp/blob.svg" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="32" cy="32" r="28"/></svg>
EOF
  cat > "$tmp/four.svg" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><circle cx="8" cy="8" r="4"/><circle cx="56" cy="8" r="4"/><circle cx="8" cy="56" r="4"/><circle cx="56" cy="56" r="4"/></svg>
EOF
  rsvg-convert -w 64 -h 64 -a "$tmp/blob.svg" -o "$tmp/blob.png"
  rsvg-convert -w 64 -h 64 -a "$tmp/four.svg" -o "$tmp/four.png"
  [[ $("${measure[@]}" ink-components "$tmp/blob.png") -eq 1 ]] \
    && ok "detector self-test: blob = 1 component" \
    || bad "detector self-test failed for blob"
  [[ $("${measure[@]}" ink-components "$tmp/four.png") -eq 4 ]] \
    && ok "detector self-test: four dots = 4 components" \
    || bad "detector self-test failed for four dots"
}

check_all() {
  echo "Detector calibration"
  self_test
  echo "Source and exact rendered geometry"
  check_source_contract
  check_geometry_box a
  check_geometry_box b
  echo "Raster component report"
  report_components a
  report_components b
  echo "Contrast"
  check_pair_contrast a "$VIOLET" "$TEAL" "$PLATE" "plate $PLATE"
  check_pair_contrast b "$TEAL" "$VIOLET" "$PLATE" "plate $PLATE"
  check_pair_contrast a "$VIOLET" "$TEAL" '#0a0a0e' "dark dock #0a0a0e"
  check_pair_contrast b "$TEAL" "$VIOLET" '#0a0a0e' "dark dock #0a0a0e"
  check_pair_contrast a "$VIOLET_LIGHT" "$TEAL_LIGHT" '#FFFFFF' "white"
  check_pair_contrast b "$TEAL_LIGHT" "$VIOLET_LIGHT" '#FFFFFF' "white"
  check_pair_contrast a "$VIOLET_LIGHT" "$TEAL_LIGHT" '#eceef5' "light ground #eceef5"
  check_pair_contrast b "$TEAL_LIGHT" "$VIOLET_LIGHT" '#eceef5' "light ground #eceef5"
  echo "Symbolic and silhouette parity"
  check_v7
  check_v8 a
  check_v8 b
  echo "Android 66dp mask"
  check_v9 android/app/src/main/res/drawable/ic_launcher_foreground_a.xml colour
  check_v9 android/app/src/main/res/drawable/ic_launcher_monochrome.xml monochrome
  echo "Palette ownership and delivery"
  check_palette_single_source
  check_delivery
  echo "Generated-file provenance"
  ./scripts/build-brand-assets.sh --check || fail=1
}

case ${1:-} in
  --all) check_all ;;
  --self-test) self_test ;;
  --mark)
    check_source_contract
    check_geometry_box "${2:-a}"
    report_components "${2:-a}"
    ;;
  *)
    printf 'usage: %s --all | --self-test | --mark [a|b]\n' "$0" >&2
    exit 2
    ;;
esac

exit "$fail"
