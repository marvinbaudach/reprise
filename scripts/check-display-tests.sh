#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

rule_named=0
case "${1:-}" in
  "") ;;
  --rule-named) rule_named=1 ;;
  *)
    echo "Usage: $0 [--rule-named]" >&2
    exit 2
    ;;
esac

mapfile -t tests < <(
  cargo test -p reprise-gnome -- --ignored --list \
    | sed -n 's/: test$//p'
)

if (( rule_named != 0 )); then
  doc=docs/ux-rules.md
  [[ -f $doc ]] || { echo "check-display-tests: $doc is missing" >&2; exit 1; }
  declare -A status_of
  while read -r id st; do
    status_of[$id]=$st
  done < <(grep -oE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(aktiv|geplant|ersetzt)' "$doc" \
    | sed -E 's/^- \*\*([A-Z]+-[0-9]+[a-z]?)\*\* \[(aktiv|geplant|ersetzt)/\1 \2/')
  prefixes=$(printf '%s\n' "${!status_of[@]}" | sed -E 's/-.*$//' \
    | sort -u | tr '[:upper:]' '[:lower:]' | paste -sd'|')
  [[ -n $prefixes ]] || { echo "check-display-tests: no rules found in $doc" >&2; exit 1; }

  rule_tests=()
  for test in "${tests[@]}"; do
    test_name=${test##*::}
    if [[ $test_name =~ ^(${prefixes})_[0-9]+[a-z]?_ ]]; then
      rule_tests+=("$test")
    fi
  done
  tests=("${rule_tests[@]}")
fi

if [[ ${#tests[@]} -eq 0 ]]; then
  echo "No ignored display tests were discovered" >&2
  exit 1
fi

for test in "${tests[@]}"; do
  data_home=$(mktemp -d)
  cache_home=$(mktemp -d)
  echo "== display test: $test =="
  dbus-run-session -- xvfb-run -a env \
    XDG_DATA_HOME="$data_home" XDG_CACHE_HOME="$cache_home" \
    GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
    cargo test -p reprise-gnome "$test" -- --ignored --exact
done
