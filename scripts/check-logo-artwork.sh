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

read -r TEAL CORAL TEAL_LIGHT CORAL_LIGHT PLATE < <(python3 - <<'PY'
import tomllib
with open("data/brand/palette.toml", "rb") as source:
    palette = tomllib.load(source)
print(*(palette[key] for key in (
    "reprise_teal", "reprise_coral", "reprise_teal_light",
    "reprise_coral_light", "reprise_plate")))
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
    "reprise-mark.svg": (palette["reprise_coral"], palette["reprise_teal"]),
    "reprise-mark-light.svg": (palette["reprise_coral_light"], palette["reprise_teal_light"]),
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
hinted = (
    ("reprise-mark-16.svg", (palette["reprise_coral"], palette["reprise_teal"])),
    ("reprise-icon-16.svg", (palette["reprise_coral"], palette["reprise_teal"])),
    ("reprise-mark-16-mono.svg", None),
)
hinted_geometry = (
    {"x": "3", "y": "5", "width": "3", "height": "3"},
    {"x": "3", "y": "9", "width": "3", "height": "3"},
    {"x": "7", "y": "3", "width": "1", "height": "10"},
    {"x": "9", "y": "3", "width": "3", "height": "10"},
)
for name, fills in hinted:
    root = ET.parse(brand / name).getroot()
    assert root.attrib["viewBox"] == "0 0 16 16", name
    shapes = list(root)
    assert len(shapes) == 4, name
    for index, (node, attributes) in enumerate(zip(shapes, hinted_geometry, strict=True)):
        assert node.tag.rsplit("}", 1)[-1] == "rect", (name, index)
        assert all(node.attrib.get(key) == value
                   for key, value in attributes.items()), (name, index)
        if fills is not None:
            assert node.attrib["fill"] == (fills[0] if index < 3 else fills[1]), (name, index)
    if fills is None:
        assert root.attrib["fill"] == "currentColor", name
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
    ok "source geometry: 96-unit sign, transparent 16-unit desktop stage, 1:3 barlines, platform plate"
  else
    bad "source geometry differs from the specified 96-unit drawing"
  fi
}

check_geometry_box() {
  local png=$tmp/geometry.png
  rsvg-convert -w 512 -h 512 -a "data/brand/reprise-mark.svg" -o "$png"
  local sx0 sy0 sx1 sy1 x0 y0 x1 y1
  read -r sx0 sy0 sx1 sy1 < <("${measure[@]}" ink-box "$png")
  read -r x0 y0 x1 y1 < <(awk -v a="$sx0" -v b="$sy0" -v c="$sx1" -v d="$sy1" \
    'BEGIN { printf "%.3f %.3f %.3f %.3f\n", a*96, b*96, c*96, d*96 }')
  if awk -v x0="$x0" -v y0="$y0" -v x1="$x1" -v y1="$y1" \
      -v t="$GEOMETRY_TOLERANCE" 'BEGIN {
        ok=(x0 >= 24.5-t && x0 <= 24.5+t && y0 >= 20-t && y0 <= 20+t &&
            x1 >= 67-t && x1 <= 67+t && y1 >= 76-t && y1 <= 76+t); exit !ok
      }'; then
    ok "V1 ink box at 512px: x=[$x0,$x1], y=[$y0,$y1] viewBox units (±1px)"
  else
    bad "V1 ink box x=[$x0,$x1], y=[$y0,$y1] misses the exact geometry"
  fi
}

# The 16px stage has its own source, so it gets its own gate rather than a
# report. Everything the 96-unit drawing cannot do at this size — four
# countable elements, each well clear of the noise floor — this one must.
check_hinted_16() {
  local png=$tmp/hinted-16.png count sizes floor smallest
  rsvg-convert -w 16 -h 16 -a data/brand/reprise-mark-16.svg -o "$png"
  count=$("${measure[@]}" ink-components "$png")
  sizes=$("${measure[@]}" ink-component-sizes "$png")
  floor=$("${measure[@]}" noise-floor "$png")
  smallest=$(awk '{print $NF}' <<<"$sizes")
  if [[ $count -eq $MARK_PARTS ]]; then
    ok "V2 16px hinted gate: $count separate components at sizes [$sizes]"
  else
    bad "V2 16px hinted gate: $count instead of $MARK_PARTS components at sizes [$sizes]"
  fi
  # Clearing the floor by a single pixel would mean the next renderer rounds it
  # away again. Demand real headroom.
  if [[ $smallest -ge $((floor * 2)) ]]; then
    ok "V2 16px hinted headroom: smallest element ${smallest}px against a ${floor}px floor"
  else
    bad "V2 16px hinted headroom: smallest element ${smallest}px is not clear of the ${floor}px floor"
  fi
  # The shipped 16px raster must come from this source, not from a downscale of
  # the 96-unit mark. Compare the two renders: identical would mean the wiring
  # silently fell back.
  rsvg-convert -w 16 -h 16 -a data/brand/reprise-mark.svg -o "$tmp/plain-16.png"
  if cmp -s "$png" "$tmp/plain-16.png"; then
    bad "V2 16px hinted source renders identically to the 96-unit mark — the stage is not wired up"
  else
    ok "V2 16px hinted source differs from the downscaled 96-unit mark"
  fi
}

report_components() {
  local size png count
  for size in 16 22 24 28 32; do
    png=$tmp/mark-$size.png
    rsvg-convert -w "$size" -h "$size" -a \
      "data/brand/reprise-mark.svg" -o "$png"
    count=$("${measure[@]}" ink-components "$png")
    if [[ $size -eq $MARK_SIZE ]]; then
      [[ $count -eq $MARK_PARTS ]] \
        && ok "V2 ${size}px gate: $count separate components" \
        || bad "V2 ${size}px gate: $count instead of $MARK_PARTS components"
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
        ok "V2 ${size}px report: the 96-unit mark puts $count of $MARK_PARTS components over the ${floor}px noise floor; all $MARK_PARTS groups survive separately at sizes [$sizes] — nothing merged, the dots are just too small to count. This stage ships from reprise-mark-16.svg instead."
      else
        ok "V2 ${size}px report: $count components above the ${floor}px noise floor; $groups pixel groups at sizes [$sizes] — strokes have run together"
      fi
    else
      ok "V2 ${size}px report: $count separate components"
    fi
  done
}

check_pair_contrast() { # <small> <large> <ground> <label>
  local small=$1 large=$2 ground=$3 label=$4 a b
  a=$("${measure[@]}" contrast "$small" "$ground")
  b=$("${measure[@]}" contrast "$large" "$ground")
  if awk -v a="$a" -v b="$b" -v floor="$MIN_CONTRAST" \
      'BEGIN { exit !(a >= floor && b >= floor) }'; then
    ok "V4 on $label: small $a:1, thick $b:1"
  else
    bad "V4 on $label: small $a:1, thick $b:1 (minimum $MIN_CONTRAST:1)"
  fi
}

check_v7() {
  local symbolic=data/icons/hicolor/symbolic/apps/io.github.marvinbaudach.Reprise-symbolic.svg
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
  for source in data/brand/reprise-mark.svg data/brand/reprise-mark-light.svg data/brand/reprise-mark-16.svg; do
    if grep -Eq '(linear|radial)Gradient' "$source"; then
      bad "V7 coloured source contains a gradient: $source"
    else
      ok "V7 coloured source is gradient-free: $source"
    fi
  done
}

check_v8_hinted() {
  local overlap
  rsvg-convert -w 256 -h 256 -a data/brand/reprise-mark-16.svg -o "$tmp/v8h-colour.png"
  rsvg-convert -w 256 -h 256 -a data/brand/reprise-mark-16-mono.svg -o "$tmp/v8h-mono.png"
  overlap=$("${measure[@]}" outline-overlap "$tmp/v8h-colour.png" "$tmp/v8h-mono.png")
  if awk -v value="$overlap" 'BEGIN { exit !(value >= 0.99) }'; then
    ok "V8 hinted colour/mono outline overlap: $overlap"
  else
    bad "V8 hinted colour/mono outline overlap: $overlap < 0.99"
  fi
}

check_v8() {
  local overlap
  rsvg-convert -w 256 -h 256 -a "data/brand/reprise-mark.svg" \
    -o "$tmp/v8-colour.png"
  rsvg-convert -w 256 -h 256 -a data/brand/reprise-mark-mono.svg \
    -o "$tmp/v8-mono.png"
  overlap=$("${measure[@]}" outline-overlap \
    "$tmp/v8-colour.png" "$tmp/v8-mono.png")
  if awk -v value="$overlap" 'BEGIN { exit !(value >= 0.99) }'; then
    ok "V8 colour/mono outline overlap: $overlap"
  else
    bad "V8 colour/mono outline overlap: $overlap < 0.99"
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
        # Prose is not a maintained source. `docs/` narrates the design and
        # `.superpowers/sdd/` is the append-only ledger of finished tasks, so a
        # colour named in one of their sentences is a quotation of history, not
        # a value anyone edits to restyle the brand -- rewriting it there would
        # falsify the record instead of removing a duplicate.
        prose = relative.parts[0] == "docs" or (
            relative.parts[:2] == (".superpowers", "sdd") and relative.suffix == ".md"
        )
        if not hits or relative == palette_path or prose:
            continue
        derived = (
            str(relative).startswith("data/icons/") or
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
  local required size
  for size in 16 22 24 32 48 64 128 256 512; do
    required="data/icons/hicolor/${size}x${size}/apps/io.github.marvinbaudach.Reprise.png"
    [[ -f $required ]] || bad "missing hicolor stage: $required"
  done
  for required in \
    data/icons/hicolor/scalable/apps/io.github.marvinbaudach.Reprise.svg \
    data/icons/hicolor/symbolic/apps/io.github.marvinbaudach.Reprise-symbolic.svg \
    data/brand/reprise-icon.svg \
    data/brand/reprise-mark.svg data/brand/reprise-mark-light.svg \
    data/brand/reprise-mark-16.svg data/brand/reprise-mark-16-mono.svg \
    data/brand/reprise-icon-16.svg \
    android/app/src/main/res/drawable/ic_repeat_sign.xml; do
    [[ -f $required ]] && ok "delivered: $required" || bad "missing: $required"
  done
  if grep -q 'transform=' data/icons/hicolor/scalable/apps/io.github.marvinbaudach.Reprise.svg; then
    bad "shipped scalable icon rescales the specified geometry"
  else
    ok "shipped scalable icon preserves the 96-unit coordinates without transform"
  fi
}

check_desktop_transparency() {
  local size icon groups
  for size in 16 22 24 32 48 64 128 256 512; do
    icon="data/icons/hicolor/${size}x${size}/apps/io.github.marvinbaudach.Reprise.png"
    groups=$("${measure[@]}" ink-component-sizes "$icon" | wc -w)
    if [[ $groups -eq $MARK_PARTS ]]; then
      ok "desktop ${size}px icon has $groups separate ink components on transparency"
    else
      bad "desktop ${size}px icon has $groups ink components instead of $MARK_PARTS — a carrier still joins the mark"
    fi
  done

  icon=$tmp/desktop-scalable.png
  rsvg-convert -w 512 -h 512 -a \
    data/icons/hicolor/scalable/apps/io.github.marvinbaudach.Reprise.svg \
    -o "$icon"
  groups=$("${measure[@]}" ink-component-sizes "$icon" | wc -w)
  if [[ $groups -eq $MARK_PARTS ]]; then
    ok "desktop scalable icon has $groups separate ink components on transparency"
  else
    bad "desktop scalable icon has $groups ink components instead of $MARK_PARTS — a carrier still joins the mark"
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
  check_geometry_box
  echo "Raster component report"
  report_components
  check_hinted_16
  echo "Contrast"
  check_pair_contrast "$CORAL" "$TEAL" "$PLATE" "plate $PLATE"
  check_pair_contrast "$CORAL" "$TEAL" '#0a0a0e' "dark dock #0a0a0e"
  check_pair_contrast "$CORAL_LIGHT" "$TEAL_LIGHT" '#FFFFFF' "white"
  check_pair_contrast "$CORAL_LIGHT" "$TEAL_LIGHT" '#eceef5' "light ground #eceef5"
  echo "Symbolic and silhouette parity"
  check_v7
  check_v8
  check_v8_hinted
  echo "Android 66dp mask"
  check_v9 android/app/src/main/res/drawable/ic_launcher_foreground.xml colour
  check_v9 android/app/src/main/res/drawable/ic_launcher_monochrome.xml monochrome
  echo "Palette ownership and delivery"
  check_palette_single_source
  check_delivery
  check_desktop_transparency
  echo "Generated-file provenance"
  ./scripts/build-brand-assets.sh --check || fail=1
}

case ${1:-} in
  --all) check_all ;;
  --desktop) check_desktop_transparency ;;
  --self-test) self_test ;;
  --mark)
    check_source_contract
    check_geometry_box "${2:-a}"
    report_components "${2:-a}"
    ;;
  *)
    printf 'usage: %s --all | --desktop | --self-test | --mark [a|b]\n' "$0" >&2
    exit 2
    ;;
esac

exit "$fail"
