#!/usr/bin/env bash
# GP-19/GP-20: the four shapes the GNOME Circle committee names as rejection
# reasons for AI-generated submissions.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/lib/rulebook.sh

src=crates

# GP-19a — comments phrased as instructions to a model.
prompt_like='^\s*//+\s*(Step [0-9]|First,|Now (we|I)|Let'"'"'s |You (should|must|can)|Note that you|As an AI|I will now|Here'"'"'s )'
n=$({ grep -rnE --include='*.rs' "$prompt_like" "$src" 2>/dev/null || true; } | wc -l)
(( n == 0 )) || report_violation GP-19 "$n comment(s) read as model instructions:
$(grep -rnE --include='*.rs' "$prompt_like" "$src" | head -10)"

# GP-19b — banner comment blocks.
banner='^\s*//\s*[=-]{10,}\s*$'
n=$({ grep -rnE --include='*.rs' "$banner" "$src" 2>/dev/null || true; } | wc -l)
(( n == 0 )) || report_violation GP-19 "$n banner comment block(s):
$(grep -rnE --include='*.rs' "$banner" "$src" | head -10)"

# GP-19c — decorative emoji in comments. The listed characters document real
# UI symbols and are not decorative prose.
emoji='^\s*//.*(?:(?![★☆✓✕⏏🗑✦])[\x{1F300}-\x{1FAFF}\x{2600}-\x{27BF}])'
n=$({ grep -rnPc --include='*.rs' "$emoji" "$src" 2>/dev/null || true; } | awk -F: '{s+=$NF} END {print s+0}')
(( n == 0 )) || report_violation GP-19 "$n comment(s) contain emoji:
$(grep -rnP --include='*.rs' "$emoji" "$src" | head -10)"

# GP-20 — dead-code allowances without a stated reason. A reason is a comment
# on the same line or on the line directly above.
allows=$(grep -rn --include='*.rs' '#\[allow(dead_code)\]' "$src" 2>/dev/null || true)
unexplained=0
while IFS= read -r hit; do
  [[ -n $hit ]] || continue
  file=${hit%%:*}
  rest=${hit#*:}
  line=${rest%%:*}
  same_line=${hit#*:*:}
  above=""
  if (( line > 1 )); then
    above=$(sed -n "$((line - 1))p" "$file")
  fi
  if [[ $same_line != *"//"* && $above != *"//"* ]]; then
    unexplained=$((unexplained + 1))
    printf '  %s:%s\n' "$file" "$line" >&2
  fi
done <<< "$allows"
(( unexplained == 0 )) || report_violation GP-20 \
  "$unexplained #[allow(dead_code)] without a stated reason (listed above)"

rulebook_exit
