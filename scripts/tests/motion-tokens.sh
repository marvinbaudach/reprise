#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT

ui_root=$fixture/crates/reprise-gnome/src/ui
mkdir -p "$ui_root/style" "$ui_root/sidebar" "$ui_root/scan" "$ui_root/window"

printf '%s\n' 'fn clean() {}' > "$ui_root/clean.rs"
MOTION_TOKEN_ROOT=$fixture "$repo_root/scripts/check-motion-tokens.sh" >/dev/null

printf '%s\n' \
  'fn bad(widget: &Widget, target: Target) {' \
  '    TimedAnimation::new(' \
  '        widget, 0.0, 1.0, 150, target,' \
  '    );' \
  '    stack.transition_duration(250);' \
  '    stack.set_transition_duration(400);' \
  '}' > "$ui_root/bad.rs"

if MOTION_TOKEN_ROOT=$fixture "$repo_root/scripts/check-motion-tokens.sh" \
    >"$fixture/out" 2>"$fixture/err"; then
  echo "motion token lint accepted literal durations" >&2
  exit 1
fi
rg --quiet 'literal animation duration.*bad.rs' "$fixture/err"

rm "$ui_root/bad.rs"
printf '%s\n' \
  'fn policy(widget: &Widget, target: Target) {' \
  '    TimedAnimation::new(widget, 0.0, 1.0, 1, target);' \
  '}' > "$ui_root/motion.rs"
printf '%s\n' 'const TRANSITION: &str = "150ms ease-out";' > "$ui_root/style/tokens.rs"
printf '%s\n' 'fn phase_two() { stack.set_transition_duration(150); }' \
  > "$ui_root/sidebar/sidebar_device_card.rs"
printf '%s\n' 'fn phase_two() { stack.transition_duration(150); }' \
  > "$ui_root/scan/scan_progress.rs"
printf '%s\n' 'fn phase_two() { stack.set_transition_duration(150); }' \
  > "$ui_root/window/window.rs"

MOTION_TOKEN_ROOT=$fixture "$repo_root/scripts/check-motion-tokens.sh" >/dev/null
echo "Motion token lint tests passed"
