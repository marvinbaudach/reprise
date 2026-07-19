#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
checker="$repo_root/scripts/check-accessibility-semantics.sh"

if [[ ! -x $checker ]]; then
  echo "$checker must exist and be executable" >&2
  exit 1
fi

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT
mkdir -p "$tmp_root/crates/reprise-gnome/src/ui"

cat >"$tmp_root/crates/reprise-gnome/src/ui/good.rs" <<'RS'
fn build(area: &gtk4::DrawingArea) {
    // a11y-semantics: role=slider name=playback-position state=value action=range-keys
    area.set_focusable(true);
    area.set_accessible_role(gtk4::AccessibleRole::Slider);
    area.update_property(&[
        gtk4::accessible::Property::Label("Playback position"),
        gtk4::accessible::Property::ValueMin(0.0),
        gtk4::accessible::Property::ValueMax(100.0),
        gtk4::accessible::Property::ValueNow(0.0),
        gtk4::accessible::Property::ValueText("0:00"),
    ]);
}
RS
A11Y_SEMANTICS_ROOT="$tmp_root" "$checker"

cat >"$tmp_root/crates/reprise-gnome/src/ui/good.rs" <<'RS'
fn build(area: &gtk4::DrawingArea) {
    area.set_focusable(true);
}
RS
if A11Y_SEMANTICS_ROOT="$tmp_root" "$checker" 2>/dev/null; then
  echo "unmarked custom focus stops must fail semantics" >&2
  exit 1
fi

cat >"$tmp_root/crates/reprise-gnome/src/ui/good.rs" <<'RS'
fn build(area: &gtk4::DrawingArea) {
    // a11y-semantics: role=slider name=playback-position state=value action=range-keys
    area.set_focusable(true);
    area.set_accessible_role(gtk4::AccessibleRole::Slider);
    area.update_property(&[gtk4::accessible::Property::Label("Playback position")]);
}
RS
if A11Y_SEMANTICS_ROOT="$tmp_root" "$checker" 2>/dev/null; then
  echo "sliders without value semantics must fail" >&2
  exit 1
fi

cat >"$tmp_root/crates/reprise-gnome/src/ui/good.rs" <<'RS'
fn build(button: &gtk4::ToggleButton) {
    button.set_accessible_role(gtk4::AccessibleRole::Tab);
}
RS
if A11Y_SEMANTICS_ROOT="$tmp_root" "$checker" 2>/dev/null; then
  echo "tabs without selected state and controls relation must fail" >&2
  exit 1
fi

echo "Accessibility semantics lint tests passed"
