#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

mode=all
case "${1:-}" in
  "") ;;
  --rule-named) mode=rule-named ;;
  # --motion runs the motion tokens' display tests (mot_* names) unconditionally,
  # i.e. without deriving prefixes from docs/ux-rules.md. Phase 1 needs this
  # because the MOT section is not committed yet, so --rule-named would filter
  # every motion test out and #[ignore] would then skip it entirely.
  --motion) mode=motion ;;
  *)
    echo "Usage: $0 [--rule-named | --motion]" >&2
    exit 2
    ;;
esac

mapfile -t tests < <(
  cargo test -p reprise-gnome -- --ignored --list \
    | sed -n 's/: test$//p'
)

if [[ $mode == motion ]]; then
  motion_tests=()
  for test in "${tests[@]}"; do
    test_name=${test##*::}
    if [[ $test_name =~ ^mot_[0-9]+[a-z]?_ ]]; then
      motion_tests+=("$test")
    fi
  done
  tests=("${motion_tests[@]}")
fi

if [[ $mode == rule-named ]]; then
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
