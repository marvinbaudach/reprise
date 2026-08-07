#!/usr/bin/env bash
# Proves the architecture lint's size rules all report before any of them
# exits. An oversized file used to abort the script before the tighter
# 600-line limits ran, so a composition root could drift over budget and stay
# invisible until an unrelated offender was fixed.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT

ui_root=$fixture/crates/reprise-gnome/src/ui
mkdir -p "$ui_root/window" "$ui_root/track_list" "$ui_root/sidebar"

lines() { seq "$1" | sed 's/.*/fn filler() {}/'; }

# Every guarded path gets a file so the lint never trips over a missing one.
lines 10 > "$ui_root/window/window.rs"
lines 10 > "$ui_root/track_list/track_list.rs"
lines 10 > "$ui_root/sidebar/sidebar.rs"

# All sizes legal: the size section must pass and the fixture run must stop
# before the workspace-dependent checks below it.
if ! ARCH_LINT_SIZE_ROOT=$fixture "$repo_root/scripts/check-architecture.sh" >/dev/null 2>&1; then
  echo "architecture lint rejected a legal fixture:" >&2
  ARCH_LINT_SIZE_ROOT=$fixture "$repo_root/scripts/check-architecture.sh" >&2 || true
  exit 1
fi

# The regression itself: one oversized ordinary file AND an oversized
# composition root. Before the fix the 800-line offender exited first and the
# window.rs breach never printed.
lines 820 > "$ui_root/big_module.rs"
lines 640 > "$ui_root/window/window.rs"
lines 610 > "$ui_root/sidebar/sidebar.rs"

both_output=$(ARCH_LINT_SIZE_ROOT=$fixture "$repo_root/scripts/check-architecture.sh" 2>&1 || true)

for expected in \
  "big_module.rs has 820 lines" \
  "window.rs has 640 lines" \
  "sidebar.rs has 610 lines"; do
  if ! grep -q "$expected" <<<"$both_output"; then
    echo "architecture lint hid a size breach behind an earlier one" >&2
    echo "expected to see: $expected" >&2
    echo "got:" >&2
    echo "$both_output" >&2
    exit 1
  fi
done

if ARCH_LINT_SIZE_ROOT=$fixture "$repo_root/scripts/check-architecture.sh" >/dev/null 2>&1; then
  echo "architecture lint exited 0 despite three size breaches" >&2
  exit 1
fi

echo "Architecture size-limit tests passed"
