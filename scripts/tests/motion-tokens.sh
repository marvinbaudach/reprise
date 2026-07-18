#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT

ui_root=$fixture/crates/reprise-gnome/src/ui
mkdir -p "$ui_root/style" "$ui_root/sidebar" "$ui_root/scan" "$ui_root/window"

# Clean file: token expressions and non-animation literals must all pass.
# - `set_duration(motion::half(...))` proves a token arg is not a literal.
# - `animation.duration()` is a getter (no args) and must never match.
# - `clamp_len(250)` is a legit non-animation literal (false-positive guard).
printf '%s\n' \
  'fn clean(animation: &TimedAnimation) {' \
  '    animation.set_duration(motion::half(motion::STANDARD));' \
  '    let _ = animation.duration();' \
  '    let x = clamp_len(250);' \
  '    let _ = x;' \
  '}' > "$ui_root/clean.rs"
MOTION_TOKEN_ROOT=$fixture "$repo_root/scripts/check-motion-tokens.sh" >/dev/null

# Bad file: every idiomatic literal spelling must be caught, including
# suffix/hex variants, a builder `.duration()`, a nested-comma constructor
# arg, and a trailing comma.
printf '%s\n' \
  'fn bad(widget: &Widget, target: Target) {' \
  '    TimedAnimation::new(' \
  '        widget, 0.0, 1.0, 150, target,' \
  '    );' \
  '    stack.transition_duration(250);' \
  '    stack.set_transition_duration(400);' \
  '    animation.set_duration(300);' \
  '    let a = adw::TimedAnimation::builder().duration(250u32).build();' \
  '    stack.set_transition_duration(0x96,);' \
  '    TimedAnimation::new(&w, mix(a, b), 1.0, 250_u32, target);' \
  '}' > "$ui_root/bad.rs"

if MOTION_TOKEN_ROOT=$fixture "$repo_root/scripts/check-motion-tokens.sh" \
    >"$fixture/out" 2>"$fixture/err"; then
  echo "motion token lint accepted literal durations" >&2
  exit 1
fi
rg --quiet 'literal animation duration.*bad.rs' "$fixture/err"
# Each variant is individually reported (proves none is masked by another).
rg --quiet 'set_duration\(300\)' "$fixture/err"          # H2 native setter
rg --quiet 'duration\(250u32\)' "$fixture/err"           # M2 builder + u32 suffix
rg --quiet 'set_transition_duration\(0x96,' "$fixture/err" # M2 hex + L1 trailing comma
rg --quiet '250_u32' "$fixture/err"                      # M2 _u32 + M3 nested-comma arg

rm "$ui_root/bad.rs"
printf '%s\n' \
  'fn policy(widget: &Widget, target: Target) {' \
  '    TimedAnimation::new(widget, 0.0, 1.0, 1, target);' \
  '}' > "$ui_root/motion.rs"
printf '%s\n' 'const TRANSITION: &str = "150ms ease-out";' > "$ui_root/style/tokens.rs"
printf '%s\n' 'fn literal_duration() { stack.set_transition_duration(150); }' \
  > "$ui_root/sidebar/sidebar_device_card.rs"
printf '%s\n' 'fn literal_duration() { stack.transition_duration(150); }' \
  > "$ui_root/scan/scan_progress.rs"
printf '%s\n' 'fn literal_duration() { stack.set_transition_duration(150); }' \
  > "$ui_root/window/window.rs"

if MOTION_TOKEN_ROOT=$fixture "$repo_root/scripts/check-motion-tokens.sh" \
    >"$fixture/out" 2>"$fixture/err"; then
  echo "motion token lint accepted literal durations in ordinary UI files" >&2
  exit 1
fi
rg --quiet 'literal animation duration.*sidebar/sidebar_device_card.rs' "$fixture/err"
rg --quiet 'literal animation duration.*scan/scan_progress.rs' "$fixture/err"
rg --quiet 'literal animation duration.*window/window.rs' "$fixture/err"

rm "$ui_root/sidebar/sidebar_device_card.rs" \
  "$ui_root/scan/scan_progress.rs" \
  "$ui_root/window/window.rs"
MOTION_TOKEN_ROOT=$fixture "$repo_root/scripts/check-motion-tokens.sh" >/dev/null
echo "Motion token lint tests passed"
