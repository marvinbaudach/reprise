#!/usr/bin/env bash
# Traceability gate: docs/ux-rules.md <-> rule-named tests.
#
# Checks three directions:
#   1. Every [aktiv] rule has >= 1 test carrying its ID in the name
#      (Rust fn snake_case or cua-e2e scenario kebab-case).
#   2. No test references an ID that is missing from the document or
#      marked [ersetzt ...].
#   3. No #[ignore] on a test whose rule is [aktiv], and every #[ignore]
#      on a rule-named test spells out "UX <ID> [geplant] — ...".
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

doc=docs/ux-rules.md
[[ -f $doc ]] || { echo "check-ux-traceability: $doc is missing" >&2; exit 1; }

fail=0

# --- Read the document: ID -> status (aktiv|geplant|ersetzt) ---
declare -A status_of
while read -r id st; do
  status_of[$id]=$st
done < <(grep -oE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(aktiv|geplant|ersetzt)' "$doc" \
  | sed -E 's/^- \*\*([A-Z]+-[0-9]+[a-z]?)\*\* \[(aktiv|geplant|ersetzt)/\1 \2/')

# Derive the section prefixes from the document itself, so a new rulebook
# section is gated without editing this script.
prefixes=$(printf '%s\n' "${!status_of[@]}" | sed -E 's/-.*$//' \
  | sort -u | tr '[:upper:]' '[:lower:]' | paste -sd'|')
[[ -n $prefixes ]] || { echo "check-ux-traceability: no rules found in $doc" >&2; exit 1; }

# --- Collect test references (snake from Rust, kebab from cua-e2e) ---
# A fn only counts when a #[test] attribute sits within the 5 lines above
# it, so plain helper fns cannot green the gate.
snake_refs=$(grep -rhA5 --include='*.rs' '#\[test\]' crates 2>/dev/null \
  | grep -oE "fn (${prefixes})_[0-9]+[a-z]?_" | sed -E 's/^fn //; s/_$//' | sort -u || true)
# Comment lines never count as coverage.
kebab_refs=$(grep -rhE "(${prefixes})-[0-9]+[a-z]?-[a-z0-9-]+" scripts/cua-e2e 2>/dev/null \
  | grep -vE '^[[:space:]]*#' \
  | grep -oE "(${prefixes})-[0-9]+[a-z]?-[a-z0-9-]+" \
  | grep -oE "^(${prefixes})-[0-9]+[a-z]?" | sort -u || true)

to_id() { # play_1a or play-1a -> PLAY-1a
  local raw=${1//-/_} prefix nr
  prefix=${raw%%_*}; nr=${raw#*_}
  printf '%s-%s' "${prefix^^}" "$nr"
}

declare -A tested
for ref in $snake_refs $kebab_refs; do
  id=$(to_id "$ref")
  tested[$id]=1
  case "${status_of[$id]:-missing}" in
    missing) echo "ERROR: test references unknown rule $id" >&2; fail=1 ;;
    ersetzt) echo "ERROR: test references replaced rule $id — re-point it" >&2; fail=1 ;;
  esac
done

# --- Direction 1: every [aktiv] rule has a test ---
for id in "${!status_of[@]}"; do
  if [[ ${status_of[$id]} == aktiv && -z ${tested[$id]:-} ]]; then
    echo "ERROR: [aktiv] rule $id has no rule-named test" >&2; fail=1
  fi
done

# --- Direction 3: no #[ignore] on [aktiv] rules, and every ignore on a ---
# --- rule-named test follows the mandated "UX <ID> [geplant] — ..." form ---
while read -r fn_name; do
  id=$(to_id "$fn_name")
  if [[ ${status_of[$id]:-} == aktiv ]]; then
    echo "ERROR: test $fn_name is ignored but rule $id is [aktiv]" >&2; fail=1
  elif ! grep -rhB3 --include='*.rs' "fn ${fn_name}_" crates 2>/dev/null \
    | grep -E '#\[ignore' | grep -qF "UX $id [geplant]"; then
    echo "ERROR: #[ignore] on $fn_name must read \"UX $id [geplant] — ...\"" >&2; fail=1
  fi
done < <(grep -rA3 --include='*.rs' '#\[ignore' crates 2>/dev/null \
  | grep -oE "fn (${prefixes})_[0-9]+[a-z]?_" | sed -E 's/^fn //; s/_$//' | sort -u || true)

if (( fail )); then exit 1; fi
active_count=$(grep -cE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[aktiv\]' "$doc" || true)
echo "UX traceability ok: $active_count active rules covered"
