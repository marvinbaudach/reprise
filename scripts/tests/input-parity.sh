#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
checker="$repo_root/scripts/check-input-parity.sh"

if [[ ! -x $checker ]]; then
  echo "$checker must exist and be executable" >&2
  exit 1
fi

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT
mkdir -p "$tmp_root/crates/reprise-gnome/src/ui"

cat >"$tmp_root/crates/reprise-gnome/src/ui/good.rs" <<'RS'
fn wire() {
    // input-parity: ACC-8 keyboard=context-menu
    let click = gtk4::GestureClick::new();
}

fn css() -> &'static str {
    ".control { outline: none; } .control:focus-visible { outline: 2px solid blue; }"
}
RS
INPUT_PARITY_ROOT="$tmp_root" "$checker"

cat >"$tmp_root/crates/reprise-gnome/src/ui/good.rs" <<'RS'
fn wire() {
    let click = gtk4::GestureClick::new();
}
RS
if INPUT_PARITY_ROOT="$tmp_root" "$checker" 2>/dev/null; then
  echo "unmarked pointer surfaces must fail input parity" >&2
  exit 1
fi

cat >"$tmp_root/crates/reprise-gnome/src/ui/good.rs" <<'RS'
fn css() -> &'static str {
    ".control { outline: none; }"
}
RS
if INPUT_PARITY_ROOT="$tmp_root" "$checker" 2>/dev/null; then
  echo "outline removal without focus-visible replacement must fail" >&2
  exit 1
fi

echo "Input parity lint tests passed"
