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
phase_two_allowlist=(
  crates/reprise-gnome/src/ui/sidebar/sidebar_device_card.rs
  crates/reprise-gnome/src/ui/scan/scan_progress.rs
  crates/reprise-gnome/src/ui/window/window.rs
)

timed_literal='TimedAnimation::new\(\s*[^,]+,\s*[^,]+,\s*[^,]+,\s*(?:0x[[:xdigit:]_]+|[0-9][0-9_]*)(?:_?u32)?\s*,'
transition_literal='(?:set_transition_duration|\.transition_duration)\(\s*(?:0x[[:xdigit:]_]+|[0-9][0-9_]*)(?:_?u32)?\s*\)'

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
  if rg --quiet --pcre2 --multiline "$timed_literal|$transition_literal" "$file"; then
    echo "ERROR: literal animation duration outside ui/motion.rs or ui/style/tokens.rs: $file" >&2
    rg --line-number --pcre2 --multiline "$timed_literal|$transition_literal" "$file" >&2 || true
    failed=1
  fi
done < <(find "$ui_root" -type f -name '*.rs' | sort)

if (( failed != 0 )); then
  exit 1
fi

echo "Motion token lint passed"
