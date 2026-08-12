#!/usr/bin/env bash
# Shared rulebook status lookup for the conformance gates.
#
# A gate reports a violation through report_violation. The violation only
# fails the build once its rule is [active] in docs/ux-rules.md; while the
# rule is [planned] it is printed as a warning. That is what lets a gate be
# wired into the merge chain before the code it guards is compliant.

RULEBOOK_DOC="${RULEBOOK_DOC:-docs/ux-rules.md}"
RULEBOOK_FAIL=0

rule_status() {
  local id=$1 line
  line=$({ grep -oE "^- \*\*${id}\*\* \[(active|planned|replaced)" "$RULEBOOK_DOC" || true; } | head -1)
  if [[ -z $line ]]; then
    printf 'missing\n'
    return
  fi
  printf '%s\n' "${line##*[}"
}

report_violation() {
  local id=$1 text=$2
  case "$(rule_status "$id")" in
    active)
      printf 'ERROR: %s — %s\n' "$id" "$text" >&2
      RULEBOOK_FAIL=1
      ;;
    planned)
      printf 'warning: %s [planned] — %s\n' "$id" "$text" >&2
      ;;
    replaced)
      printf 'ERROR: gate references replaced rule %s — re-point it\n' "$id" >&2
      RULEBOOK_FAIL=1
      ;;
    *)
      printf 'ERROR: gate references unknown rule %s\n' "$id" >&2
      RULEBOOK_FAIL=1
      ;;
  esac
}

skip_gate() {
  printf 'SKIPPED: %s\n' "$1" >&2
  exit 0
}

skip_gate_if_tool_missing() {
  local tool=$1
  command -v "$tool" >/dev/null 2>&1 && return 0
  skip_gate "$tool is not installed; this gate did not run"
}

rulebook_exit() {
  (( RULEBOOK_FAIL )) && exit 1
  exit 0
}
