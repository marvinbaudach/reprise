#!/usr/bin/env bash
# MOT-1 gate: app-authored widget transitions use named motion tokens.
set -euo pipefail

repo_root=${MOTION_TOKEN_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
cd "$repo_root"

ui_root=crates/reprise-gnome/src/ui
[[ -d $ui_root ]] || {
  echo "check-motion-tokens: $ui_root is missing" >&2
  exit 1
}

# The token implementation and its raw-constructor contract test define the
# policy, while the CSS token adapter has to render the numeric duration.
policy_files=(
  crates/reprise-gnome/src/ui/motion.rs
  crates/reprise-gnome/src/ui/style/tokens.rs
)

# Phase 2 -- migrated in T4/T5 and removed from this allowlist.
# missing_progress.rs is missing-import-errors territory (still an open branch);
# its background progress-card revealer is migrated alongside scan_progress.rs
# once that work lands, not from this branch.
phase_two_allowlist=(
  crates/reprise-gnome/src/ui/sidebar/sidebar_device_card.rs
  crates/reprise-gnome/src/ui/scan/scan_progress.rs
  crates/reprise-gnome/src/ui/issues/missing_progress.rs
)

# --- Literal-duration detection (heuristic; see the limits below) ---
#
# The lint fails when an app-authored animation duration is written as a bare
# integer literal outside the token module. Three idiomatic spellings are
# covered:
#   1. `TimedAnimation::new(w, from, to, <LIT>, target)` — 4th positional arg.
#   2. `set_transition_duration(<LIT>)` / `.transition_duration(<LIT>)`.
#   3. `.set_duration(<LIT>)` (the native Adw duration setter) and the builder
#      form `.duration(<LIT>)`.
# The *literal* is the signal: token expressions such as
# `set_duration(motion::half(motion::STANDARD))` are never literals and never
# match, so the migrated (token-based) call sites stay green.
#
# Known, deliberate limits — this is a heuristic, not an airtight parser
# (G6 permits catching the idiomatic literal spellings and leaving the rest to
# review):
#   * Variable indirection (`let d = 250; anim.set_duration(d);`) is NOT
#     caught — that path is a review responsibility, not a lint one.
#   * A cast literal (`250 as u32`) is not caught.
#   * `TimedAnimation::new` matching tolerates one level of nested parens in the
#     value_from / value_to / target arguments (so `f(a, b)` no longer shifts
#     the positional count); deeper nesting is out of scope.
#   * `.duration(<LIT>)` / `.set_duration(<LIT>)` are assumed to be Adw
#     animation durations. No non-animation `.duration(<literal>)` exists in
#     ui/ today (getters take no args, the waveform track duration takes a
#     variable); a future literal on a non-animation type would be a false
#     positive to allowlist.
lit='(?:0x[[:xdigit:]_]+|[0-9][0-9_]*)(?:_?u32)?'
arg='[^,()]*(?:\([^()]*\)[^,()]*)*'
timed_literal="TimedAnimation::new\(\s*${arg},\s*${arg},\s*${arg},\s*${lit}\s*,"
transition_literal="(?:set_transition_duration|\.transition_duration)\(\s*${lit}\s*(?:,\s*)?\)"
duration_literal="\.(?:set_duration|duration)\(\s*${lit}\s*(?:,\s*)?\)"

is_allowlisted() {
  local candidate=$1 allowed
  for allowed in "${policy_files[@]}" "${phase_two_allowlist[@]}"; do
    if [[ $candidate == "$allowed" ]]; then
      return 0
    fi
  done
  return 1
}

failed=0
while IFS= read -r file; do
  if is_allowlisted "$file"; then
    continue
  fi
  if rg --quiet --pcre2 --multiline "$timed_literal|$transition_literal|$duration_literal" "$file"; then
    echo "ERROR: literal animation duration outside ui/motion.rs or ui/style/tokens.rs: $file" >&2
    rg --line-number --pcre2 --multiline "$timed_literal|$transition_literal|$duration_literal" "$file" >&2 || true
    failed=1
  fi
done < <(find "$ui_root" -type f -name '*.rs' | sort)

if (( failed != 0 )); then
  exit 1
fi

echo "Motion token lint passed"
