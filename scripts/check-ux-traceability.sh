#!/usr/bin/env bash
# Traceability gate: docs/ux-rules.md <-> rule-named tests.
#
# Checks three directions:
#   1. Every [active] rule has >= 1 test carrying its ID in the name
#      (Rust fn snake_case or cua-e2e scenario kebab-case).
#   2. No test references an ID that is missing from the document or
#      marked [replaced ...].
#   3. The display-runner marker is allowed on every rule status. Every other
#      #[ignore] is limited to [planned] rules and must spell out
#      "UX <ID> [planned] — ...".
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

doc=docs/ux-rules.md
[[ -f $doc ]] || { echo "check-ux-traceability: $doc is missing" >&2; exit 1; }

fail=0

# --- Read the document: ID -> status (active|planned|replaced) ---
declare -A status_of
while read -r id st; do
  status_of[$id]=$st
done < <(grep -oE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(active|planned|replaced)' "$doc" \
  | sed -E 's/^- \*\*([A-Z]+-[0-9]+[a-z]?)\*\* \[(active|planned|replaced)/\1 \2/')

# --- Read the document: ID -> level (core|gtk|e2e|manual) ---
declare -A level_of
while read -r id lvl; do
  level_of[$id]=$lvl
done < <(grep -oE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(active|planned)\] \[(core|gtk|e2e|manual)\]' "$doc" \
  | sed -E 's/^- \*\*([A-Z]+-[0-9]+[a-z]?)\*\* \[(active|planned)\] \[([a-z0-9]+)\]/\1 \3/')

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

# --- Collect checklist references (RELEASING.md, word-bounded IDs) ---
releasing=RELEASING.md
prefixes_upper=$(printf '%s' "$prefixes" | tr '[:lower:]' '[:upper:]')
declare -A in_releasing
while read -r id; do
  [[ -n $id ]] || continue
  in_releasing[$id]=1
  case "${status_of[$id]:-missing}" in
    missing) echo "ERROR: RELEASING.md references unknown rule $id" >&2; fail=1 ;;
    replaced) echo "ERROR: RELEASING.md references replaced rule $id — re-point it" >&2; fail=1 ;;
  esac
done < <(grep -hoE "\b(${prefixes_upper})-[0-9]+[a-z]?\b" "$releasing" 2>/dev/null | sort -u || true)

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
    replaced) echo "ERROR: test references replaced rule $id — re-point it" >&2; fail=1 ;;
  esac
done

# --- Direction 1: every [active] rule is covered on its level ---
for id in "${!status_of[@]}"; do
  [[ ${status_of[$id]} == active ]] || continue
  if [[ ${level_of[$id]:-} == manual ]]; then
    if [[ -z ${in_releasing[$id]:-} ]]; then
      echo "ERROR: [active] [manual] rule $id is not referenced in RELEASING.md" >&2; fail=1
    fi
  elif [[ -z ${tested[$id]:-} ]]; then
    echo "ERROR: [active] rule $id has no rule-named test" >&2; fail=1
  fi
done

# --- Direction 3: display-runner markers are coverage, other ignores are ---
# --- limited to [planned] rules and the "UX <ID> [planned] — ..." form ---
while read -r fn_name; do
  ref=$(printf '%s' "$fn_name" | grep -oE "^(${prefixes})_[0-9]+[a-z]?")
  id=$(to_id "$ref")
  ignore_lines=$(grep -rhB3 --include='*.rs' "fn ${fn_name}(" crates 2>/dev/null \
    | grep -E '^[[:space:]]*#\[ignore' || true)
  if printf '%s\n' "$ignore_lines" \
    | grep -qE '^[[:space:]]*#\[ignore = "requires a display; run via xvfb-run"\][[:space:]]*$'; then
    continue
  fi
  if [[ ${status_of[$id]:-} == active ]]; then
    echo "ERROR: test $ref is ignored but rule $id is [active]" >&2; fail=1
  elif ! printf '%s\n' "$ignore_lines" | grep -qF "UX $id [planned]"; then
    echo "ERROR: #[ignore] on $ref must read \"UX $id [planned] — ...\"" >&2; fail=1
  fi
done < <(grep -rA3 --include='*.rs' '#\[ignore' crates 2>/dev/null \
  | grep -oE "fn (${prefixes})_[0-9]+[a-z]?_[a-z0-9_]+" | sed -E 's/^fn //' | sort -u || true)

if (( fail )); then exit 1; fi
active_count=$(grep -cE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[active\]' "$doc" || true)
echo "UX traceability ok: $active_count active rules covered"
