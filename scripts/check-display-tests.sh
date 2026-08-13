#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

mode=all
case "${1:-}" in
  "") ;;
  --rule-named) mode=rule-named ;;
  # --css runs only ignored CSS-provider parsing guards. Keep this targeted:
  # it is a focused developer mode, never the standing merge gate.
  --css) mode=css ;;
  *)
    echo "Usage: $0 [--rule-named | --css]" >&2
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

if [[ $mode == rule-named ]]; then
  doc=docs/ux-rules.md
  [[ -f $doc ]] || { echo "check-display-tests: $doc is missing" >&2; exit 1; }
  declare -A status_of
  while read -r id st; do
    status_of[$id]=$st
  done < <(grep -oE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(active|planned|replaced)' "$doc" \
    | sed -E 's/^- \*\*([A-Z]+-[0-9]+[a-z]?)\*\* \[(active|planned|replaced)/\1 \2/')
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

# The display band this run owns (see `server_num` below). One band per run,
# 4000 wide, picked from this shell's PID: enough headroom for the test count
# plus both retry steps, and cheap. Two runs whose PIDs land in the same bucket
# still collide — a one-in-sixteen chance that costs a retry, not a wrong
# result — which is the trade for not maintaining a lock file of claimed bands.
run_display_offset=$(( ($$ % 16) * 4000 ))

cleanup_worker_roots() {
  # Portal and accessibility helpers can release private mounts slightly
  # after the test process exits. Cleanup must not overwrite the recorded
  # test result or abort the remaining display-test balance sheet.
  rm -rf "$@" 2>/dev/null || true
}

run_display_test() {
  local index=$1
  local test=$2
  local data_home cache_home config_home runtime_dir marker_dir tmp_home
  local display_test_passed display_test_output server_num attempt attempts
  # The parent owns the shared results directory. A background worker must
  # never inherit its EXIT cleanup and remove siblings' logs or statuses.
  trap - EXIT
  data_home=$(mktemp -d)
  cache_home=$(mktemp -d)
  config_home=$(mktemp -d)
  runtime_dir=$(mktemp -d)
  chmod 700 "$runtime_dir"
  marker_dir=$(mktemp -d)
  # The sixth directory, and the one the cleanup below could not reach until
  # now. The test process makes its own fixture root through `tempfile`
  # (`reprise-gnome/src/test_db.rs`, prefix `reprise-gnome-tests-`) and holds
  # it in a `static OnceLock<TempDir>`. Rust never drops statics, so that
  # directory outlives *every* exit — a passing run leaks exactly as reliably
  # as a killed one. Measured on 2026-07-30: one clean 218-test run left 243
  # directories and 905 MB behind, the largest 90 MB each, and an earlier
  # accumulation of 926 of them (7.1 GB of the 16 GB tmpfs, i.e. RAM) made a
  # full run report 153 of 217 tests as display failures when the real cause
  # was "No space left on device".
  #
  # Fixing it in the fixture would mean giving up the static that deliberately
  # outlives every test in the process, so it is fixed here instead: TMPDIR
  # points at a worker-owned directory, which puts the fixture root inside the
  # tree the trap below already removes — on exit, interrupt, or kill alike.
  tmp_home=$(mktemp -d)
  # Own the cleanup rather than merely reaching it. The `trap - EXIT` above
  # drops the PARENT's cleanup so a worker never deletes its siblings' logs;
  # it must not leave the worker with no cleanup at all. Without this, the
  # tidy-up below runs only on the normal path, so every interrupted or
  # timed-out gate run abandons five directories per test — about 955 for a
  # full run. That accumulated to 8450 directories and 15G in a 16G tmpfs,
  # i.e. in RAM, which is the same trap AGENTS.md already warns about for
  # stray build directories.
  # INT/TERM/HUP as well as EXIT: a killed or timed-out run is precisely the
  # case that leaked, and bash does not run an EXIT trap for an untrapped
  # fatal signal.
  trap 'cleanup_worker_roots "$data_home" "$cache_home" "$config_home" \
    "$runtime_dir" "$marker_dir" "$tmp_home"' EXIT INT TERM HUP
  display_test_passed="$marker_dir/passed"
  # xvfb-run -a can race while parallel workers probe the same free display.
  # Assign a stable, unique server number to every worker instead.
  #
  # Unique within this run, and — through `run_display_offset` — across
  # concurrent runs too. `99 + index` alone is identical in every invocation of
  # this script, so two gate runs started from two worktrees claim exactly the
  # same display for every index; `xvfb-run --server-num` refuses a number
  # whose /tmp/.X<n>-lock exists, and the loser reports "display never came up"
  # for a stretch of neighbouring tests. That reads as a code defect and is not
  # one. Retries add 1000 and 2000, so a run needs its own band wider than
  # that plus the test count.
  server_num=$((99 + index + run_display_offset))
  # Under load Xvfb can still be coming up when the test connects, which
  # surfaces as GTK's "Failed to initialize GTK" rather than as a test failure.
  # That signature is an environment fault, never an assertion, so it earns a
  # retry on a different server number. Any other failure is reported as-is.
  #
  # Two retries, not one: on a machine running several agents at once, two
  # consecutive full runs on 2026-07-30 each lost the same contiguous block of
  # four `ui::podcasts::*` tests, every one of them to this signature and to
  # the script's own "display never came up", and every one of them passing in
  # isolation immediately afterwards. Neighbouring tests fail together because
  # the window of contention is a stretch of wall-clock time, which is also why
  # a repeated run looks deterministic and invites the wrong conclusion. One
  # retry was not enough to cross that window; the cost of a third attempt is
  # paid only by a test that has already failed twice.
  attempts=3
  {
    echo "== display test: $test =="
    # Set XDG roots before dbus-run-session so D-Bus-activated Portal and
    # AT-SPI services inherit the worker isolation too.
    # Xvfb has no usable GPU here. Force Cairo so a failed Vulkan probe cannot
    # consume an animation's timing window before the first assertion.
    # xvfb-run can return non-zero after the test process succeeded when its
    # cleanup races an already-exited Xvfb process. The marker is written only
    # after cargo reports success and at least one test binary reports exactly
    # one passing test, so it remains the authoritative result. `--exact` with
    # a stale name exits zero after running nothing; that is a gate failure.
    for ((attempt = 1; attempt <= attempts; attempt++)); do
      display_test_output="$marker_dir/attempt-$attempt.log"
      if env \
        XDG_DATA_HOME="$data_home" XDG_CACHE_HOME="$cache_home" \
        XDG_CONFIG_HOME="$config_home" XDG_RUNTIME_DIR="$runtime_dir" \
        TMPDIR="$tmp_home" \
        GIO_USE_VFS=local GTK_USE_PORTAL=0 \
        GSK_RENDERER=cairo \
        GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
        DISPLAY_TEST="$test" DISPLAY_TEST_PASSED="$display_test_passed" \
        DISPLAY_TEST_OUTPUT="$display_test_output" \
        dbus-run-session -- xvfb-run --server-num="$server_num" \
        bash -c '
          set -o pipefail
          if ! cargo test -p reprise-gnome "$DISPLAY_TEST" -- --ignored --exact \
            2>&1 | tee "$DISPLAY_TEST_OUTPUT"; then
            exit 1
          fi
          passed_lines=$(grep -Ec "test result: ok\\. 1 passed;" \
            "$DISPLAY_TEST_OUTPUT" || true)
          if (( passed_lines < 1 )); then
            echo "display test matched no executing test binary: $DISPLAY_TEST" >&2
            exit 1
          fi
          : >"$DISPLAY_TEST_PASSED"
        '; then
        :
      fi
      # An explicit if: under `set -e` a bare `[[ ... ]] && break` would abort
      # the worker on the common case, before the status is ever written.
      if [[ -f $display_test_passed ]]; then
        break
      fi
      (( attempt < attempts )) || break
      # Only the display never coming up is retried; a real failure stands.
      grep -q "Failed to initialize GTK" "$results_dir/$index.log" 2>/dev/null \
        || break
      server_num=$((server_num + 1000))
      echo "== retrying $test on server :$server_num (display never came up) =="
    done
    if [[ -f $display_test_passed ]]; then
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
