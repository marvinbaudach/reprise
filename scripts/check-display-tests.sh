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
  # --css runs only ignored CSS-provider parsing guards. Keep this targeted:
  # unrelated display tests remain owned by their rule or motion gates.
  --css) mode=css ;;
  *)
    echo "Usage: $0 [--rule-named | --motion | --css]" >&2
    exit 2
    ;;
esac

mapfile -t tests < <(
  cargo test -p reprise-gnome -- --ignored --list \
    | sed -n 's/: test$//p'
)

if [[ $mode == css ]]; then
  css_tests=()
  for test in "${tests[@]}"; do
    test_name=${test##*::}
    if [[ $test_name =~ css.*pars ]]; then
      css_tests+=("$test")
    fi
  done
  tests=("${css_tests[@]}")
fi

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

jobs=${DISPLAY_TEST_JOBS:-1}
if [[ ! $jobs =~ ^[1-9][0-9]*$ ]]; then
  echo "DISPLAY_TEST_JOBS must be a positive integer" >&2
  exit 2
fi

# Every test runs, whatever the ones before it did. A fail-fast loop reports
# the first red test and hides how many others are red — which is exactly the
# information needed to judge whether the suite is trustworthy at all. Failures
# are collected and reported in one balance sheet at the end instead. Each
# worker owns its XDG roots, D-Bus session, X server, marker, and log. Keeping
# the default at one preserves the local debugging order; CI opts into a small
# bounded pool through DISPLAY_TEST_JOBS.
results_dir=$(mktemp -d)
trap 'rm -rf "$results_dir"' EXIT

cleanup_worker_roots() {
  # Portal and accessibility helpers can release private mounts slightly
  # after the test process exits. Cleanup must not overwrite the recorded
  # test result or abort the remaining display-test balance sheet.
  rm -rf "$@" 2>/dev/null || true
}

run_display_test() {
  local index=$1
  local test=$2
  local data_home cache_home config_home runtime_dir marker_dir
  local display_test_passed server_num
  # The parent owns the shared results directory. A background worker must
  # never inherit its EXIT cleanup and remove siblings' logs or statuses.
  trap - EXIT
  data_home=$(mktemp -d)
  cache_home=$(mktemp -d)
  config_home=$(mktemp -d)
  runtime_dir=$(mktemp -d)
  chmod 700 "$runtime_dir"
  marker_dir=$(mktemp -d)
  display_test_passed="$marker_dir/passed"
  # xvfb-run -a can race while parallel workers probe the same free display.
  # Assign a stable, unique server number to every worker instead.
  server_num=$((99 + index))
  {
    echo "== display test: $test =="
    # Set XDG roots before dbus-run-session so D-Bus-activated Portal and
    # AT-SPI services inherit the worker isolation too.
    if env \
      XDG_DATA_HOME="$data_home" XDG_CACHE_HOME="$cache_home" \
      XDG_CONFIG_HOME="$config_home" XDG_RUNTIME_DIR="$runtime_dir" \
      GIO_USE_VFS=local GTK_USE_PORTAL=0 \
      GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
      DISPLAY_TEST="$test" DISPLAY_TEST_PASSED="$display_test_passed" \
      dbus-run-session -- xvfb-run --server-num="$server_num" \
      bash -c '
        cargo test -p reprise-gnome "$DISPLAY_TEST" -- --ignored --exact \
          && : >"$DISPLAY_TEST_PASSED"
      ' && [[ -f $display_test_passed ]]; then
      echo pass >"$results_dir/$index.status"
    else
      echo fail >"$results_dir/$index.status"
    fi
  } >"$results_dir/$index.log" 2>&1
  cleanup_worker_roots "$data_home" "$cache_home" "$config_home" \
    "$runtime_dir" "$marker_dir"
}

active=0
for index in "${!tests[@]}"; do
  run_display_test "$index" "${tests[$index]}" &
  active=$((active + 1))
  if (( active >= jobs )); then
    wait -n || true
    active=$((active - 1))
  fi
done
wait || true

passed=0
failed_tests=()
for index in "${!tests[@]}"; do
  cat "$results_dir/$index.log"
  if [[ -f $results_dir/$index.status ]] \
    && [[ $(<"$results_dir/$index.status") == pass ]]; then
    passed=$((passed + 1))
  else
    failed_tests+=("${tests[$index]}")
  fi
done

echo
echo "== display test summary =="
echo "passed: $passed"
echo "failed: ${#failed_tests[@]} of ${#tests[@]}"

if (( ${#failed_tests[@]} > 0 )); then
  printf '  %s\n' "${failed_tests[@]}"
  exit 1
fi
