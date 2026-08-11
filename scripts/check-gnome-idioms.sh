#!/usr/bin/env bash
# GP-2/GP-3/GP-4: gtk4-rs idioms in the GTK frontend.
#
# This gate greps. It is a tripwire, not a proof: it catches the shapes that
# reviewers reject, and it reports counts so a rule can be switched to
# [active] once the count reaches zero.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/lib/rulebook.sh

ui=crates/reprise-gnome/src
[[ -d $ui ]] || { echo "ERROR: $ui does not exist" >&2; exit 1; }

count_matches() { grep -rnE --include='*.rs' "$1" "$ui" 2>/dev/null | grep -vcE '^\s*//' || true; }
list_matches()  { grep -rnE --include='*.rs' "$1" "$ui" 2>/dev/null | grep -vE '^\s*//' | head -10 || true; }

# GP-2 — blocking calls that must not sit on the main loop.
blocking='(std::thread::sleep|\.blocking_recv\(\)|\.blocking_send\(|block_on\()'
n=$(count_matches "$blocking")
(( n == 0 )) || report_violation GP-2 "$n blocking call(s) in $ui:
$(list_matches "$blocking")"

# GP-3 — clone! blocks that capture without #[weak] or #[weak_allow_none].
n=$(grep -rn --include='*.rs' -A2 'clone!(' "$ui" 2>/dev/null \
  | grep -E '#\[strong\]' | grep -vcE '^\s*//' || true)
(( n == 0 )) || report_violation GP-3 "$n clone! block(s) capture strongly:
$(grep -rn --include='*.rs' -A2 'clone!(' "$ui" | grep -E '#\[strong\]' | head -10)"

# GP-4 — unwrap() in the frontend.
n=$(count_matches '\.unwrap\(\)')
(( n == 0 )) || report_violation GP-4 "$n unwrap() call(s) in $ui:
$(list_matches '\.unwrap\(\)')"

rulebook_exit
