#!/usr/bin/env bash
set -euo pipefail

# Resolve the repo root from this script's own location rather than from git:
# the sharded CI job runs the script directly inside a container where git
# refuses the workspace as dubious ownership, and the sibling gates that do call
# git (.github/scripts/check-gnome-ci.sh, scripts/ci-quality.sh) only reach it
# after their own safe.directory setup.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

mode=all
list_only=false
shard_spec=

usage() {
  echo "Usage: $0 [--rule-named | --css] [--shard K/N] [--list]" >&2
  exit 2
}

while (( $# > 0 )); do
  case $1 in
    --rule-named)
      [[ $mode == all ]] || usage
      mode=rule-named
      shift
      ;;
    # --css runs only ignored CSS-provider parsing guards. Keep this targeted:
    # it is a focused developer mode, never the standing merge gate.
    --css)
      [[ $mode == all ]] || usage
      mode=css
      shift
      ;;
    --shard) shard_spec=${2:-}
      [[ -n $shard_spec ]] || usage
      shift 2
      ;;
    --list) list_only=true
      shift
      ;;
    *) usage ;;
  esac
done

shard_index=
shard_count=
if [[ -n $shard_spec ]]; then
  [[ $shard_spec =~ ^([1-9][0-9]*)/([1-9][0-9]*)$ ]] || usage
  shard_index=${BASH_REMATCH[1]}
  shard_count=${BASH_REMATCH[2]}
  (( shard_index <= shard_count )) || usage
fi

# One directory owns everything this run leaves behind: the build's artifact
# stream, and each worker's log and status. The trap is set once, here, because
# a second `trap ... EXIT` further down would replace this one.
results_dir=$(mktemp -d)
trap 'rm -rf "$results_dir"' EXIT

command -v jq >/dev/null \
  || { echo "check-display-tests: jq is required to find the test binary" >&2; exit 1; }

# The cargo selection that builds the test binaries. It is exactly the merge
# gate's "Workspace tests" selection (the `gate "Workspace tests"` line of
# scripts/check-merge-readiness.sh), so inside that gate this build finds every
# artifact fresh and compiles nothing. A narrower `-p reprise-gnome` would
# switch on a different feature set (dev-dependencies of other workspace
# members unify differently) and recompile dozens of crates a second time.
# scripts/tests/qa-linters.sh fails when the two selections drift apart.
workspace_test_selection=(--locked --workspace --exclude reprise-platform-linux)

# Build once. Every display test below executes the resulting binary directly:
# going through `cargo test` per test costs a cargo start-up and a second test
# target (tests/gnome_conformance.rs) each time, and none of the ignored tests
# live there. Compiler diagnostics are rendered to stderr by cargo itself;
# stdout carries only the artifact stream. A failed build must stop the script
# here, not leave an empty test list behind.
build_json="$results_dir/build.json"
if ! cargo test "${workspace_test_selection[@]}" --no-run \
  --message-format=json-render-diagnostics >"$build_json"; then
  echo "check-display-tests: building the workspace test binaries failed" >&2
  exit 1
fi

# The `reprise` bin's unit-test target holds every ignored display test. The
# integration test target and the non-test `reprise` bin are different
# artifacts and must not match, so the filter names all of: the package (by
# manifest path, not by a substring of its id), the bin, and `profile.test`.
mapfile -t test_bins < <(
  jq -r 'select(.reason == "compiler-artifact"
                and .profile.test == true
                and .target.name == "reprise"
                and (.target.kind | index("bin"))
                and (.manifest_path | endswith("/crates/reprise-gnome/Cargo.toml"))
                and .executable != null)
         | .executable' "$build_json" | sort -u
)
if (( ${#test_bins[@]} != 1 )); then
  echo "check-display-tests: expected exactly one reprise-gnome test binary," \
    "found ${#test_bins[@]}" >&2
  printf '  %s\n' "${test_bins[@]}" >&2
  exit 1
fi
test_bin=${test_bins[0]}
if [[ ! -x $test_bin ]]; then
  echo "check-display-tests: $test_bin is not an executable file" >&2
  exit 1
fi

if ! listing=$("$test_bin" --ignored --list); then
  echo "check-display-tests: listing the ignored tests of $test_bin failed" >&2
  exit 1
fi
mapfile -t tests < <(
  printf '%s\n' "$listing" \
    | sed -n 's/: test$//p' \
    | sort
)

# `--ignored` is how a display test announces that it needs an X server, but it
# is also how a measurement tool hides from the ordinary suite. Such a tool
# takes its input from the operator through environment variables and panics
# without it, so running it here reports a red that says nothing about the
# product. Tools are dropped by their own declared reason —
# `#[ignore = "measurement: ..."]` — never by a hand-kept list of names, which
# would rot the moment someone adds the next one. Every drop is named below: a
# suite that quietly shrinks is worse than a suite that is red.
mapfile -t measurement_sources < <(
  grep -rl '#\[ignore = "measurement' crates/reprise-gnome/src || true
)
mapfile -t measurement_tools < <(
  if (( ${#measurement_sources[@]} > 0 )); then
    awk '/#\[ignore = "measurement/ { want = 1; next }
         want && match($0, /fn [a-z0-9_]+/) {
           print substr($0, RSTART + 3, RLENGTH - 3)
           want = 0
         }' "${measurement_sources[@]}"
  fi
)

skipped=()
if (( ${#measurement_tools[@]} > 0 )); then
  kept=()
  for test in "${tests[@]}"; do
    is_tool=
    for tool in "${measurement_tools[@]}"; do
      if [[ ${test##*::} == "$tool" ]]; then
        is_tool=1
        break
      fi
    done
    if [[ -n $is_tool ]]; then
      skipped+=("$test")
    else
      kept+=("$test")
    fi
  done
  tests=("${kept[@]}")
fi

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

if [[ -n $shard_spec ]]; then
  shard_tests=()
  for index in "${!tests[@]}"; do
    if (( index % shard_count == shard_index - 1 )); then
      shard_tests+=("${tests[$index]}")
    fi
  done
  tests=("${shard_tests[@]}")
fi

if [[ ${#tests[@]} -eq 0 ]]; then
  echo "No ignored display tests were discovered" >&2
  exit 1
fi

if [[ $list_only == true ]]; then
  printf '%s\n' "${tests[@]}"
  exit 0
fi

for tool in Xvfb dbus-run-session; do
  command -v "$tool" >/dev/null \
    || { echo "check-display-tests: $tool is required to run display tests" >&2; exit 1; }
done

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

# Seconds a freshly started Xvfb gets to report which display it picked. It
# normally answers in a few hundred milliseconds; a machine running several
# agents at once can stretch that, but a server silent for this long is not
# coming up.
x_display_timeout_s=10

# The Xvfb process of the current worker. Each worker is its own subshell, so
# this global is private to it; being global rather than a function local, the
# signal traps below still see it after `run_display_test` has returned.
worker_xvfb_pid=
worker_display=

# Starts this worker's private X server and says which display it got.
#
# Xvfb is asked for `-displayfd`: it picks a free display number itself and
# writes it to that descriptor once it is ready to accept connections. That
# replaces guessing a display number per test (and per concurrent run) and
# replaces the fixed start-up sleep of `xvfb-run`, so a test starts the moment
# its server is up. The 640x480x24 screen is `xvfb-run`'s default; geometry
# dependent tests rely on it. No TCP, no X authority file: the server is
# reachable through its local socket only.
#
# Returns 1, with the server stopped, when no display number arrived in time or
# the server exited first. That, and only that, is an environment fault worth a
# retry: it says nothing about the test.
start_worker_xvfb() {
  local fifo=$1
  local log=$2
  local fd number waited=0
  mkfifo "$fifo"
  # Read-write, so opening never blocks waiting for the other end and the read
  # below sees no end-of-file while the server is still starting.
  exec {fd}<>"$fifo"
  Xvfb -displayfd 3 -screen 0 640x480x24 -nolisten tcp \
    3>&"$fd" </dev/null >/dev/null 2>"$log" &
  worker_xvfb_pid=$!
  worker_display=
  while (( waited < x_display_timeout_s )); do
    if read -r -t 1 -u "$fd" number; then
      if [[ $number =~ ^[0-9]+$ ]]; then
        worker_display=$number
      fi
      break
    fi
    # Gone without a word: no point in waiting out the rest of the timeout.
    kill -0 "$worker_xvfb_pid" 2>/dev/null || break
    waited=$((waited + 1))
  done
  exec {fd}<&-
  if [[ -z $worker_display ]]; then
    stop_worker_xvfb
    return 1
  fi
}

stop_worker_xvfb() {
  [[ -n $worker_xvfb_pid ]] || return 0
  kill "$worker_xvfb_pid" 2>/dev/null || true
  # Reaped, so no zombie is left behind and its display socket and lock file
  # are gone before the next test asks for a display.
  wait "$worker_xvfb_pid" 2>/dev/null || true
  worker_xvfb_pid=
}

cleanup_worker_roots() {
  # The worker's X server goes first: it lives under these roots' lifetime and
  # must not outlive an interrupted run.
  stop_worker_xvfb
  # Portal and accessibility helpers can release private mounts slightly
  # after the test process exits. Cleanup must not overwrite the recorded
  # test result or abort the remaining display-test balance sheet.
  rm -rf "$@" 2>/dev/null || true
}

run_display_test() {
  local index=$1
  local test=$2
  local data_home cache_home config_home runtime_dir marker_dir tmp_home
  local display_test_passed display_test_output attempt attempts
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
  # Only an X server that never reported a display is retried. That happens
  # when a machine running several agents at once is starved for a moment —
  # two consecutive full runs on 2026-07-30 each lost the same contiguous block
  # of four `ui::podcasts::*` tests to it, every one of them passing in
  # isolation immediately afterwards. Neighbouring tests fail together because
  # the window of contention is a stretch of wall-clock time, which is also why
  # a repeated run looks deterministic and invites the wrong conclusion. One
  # retry was not enough to cross that window; the cost of a third attempt is
  # paid only by a test whose server has already failed to start twice.
  #
  # A server that did report its display is a working server: from there on a
  # "Failed to initialize GTK" or any other failure is the test's own result and
  # stands. Retrying that would be blind, and would hide real flakiness.
  attempts=3
  {
    echo "== display test: $test =="
    # Set XDG roots before the D-Bus session starts so D-Bus-activated Portal
    # and AT-SPI services inherit the worker isolation too.
    # Xvfb has no usable GPU here. Force Cairo so a failed Vulkan probe cannot
    # consume an animation's timing window before the first assertion.
    # The marker is written only after the test binary reports success and
    # exactly one passing test, so it remains the authoritative result.
    # `--exact` with a stale name exits zero after running nothing; that is a
    # gate failure.
    for ((attempt = 1; attempt <= attempts; attempt++)); do
      display_test_output="$marker_dir/attempt-$attempt.log"
      if ! start_worker_xvfb "$marker_dir/display-$attempt" \
        "$marker_dir/xvfb-$attempt.log"; then
        echo "== Xvfb reported no display within ${x_display_timeout_s}s" \
          "(attempt $attempt of $attempts) =="
        sed 's/^/xvfb: /' "$marker_dir/xvfb-$attempt.log" || true
        continue
      fi
      # The unit tests run with the package root as their working directory
      # under `cargo test`; running the binary directly keeps that. Nothing
      # read at run time comes from the CARGO_* variables cargo exports (every
      # use is a compile-time `env!`), so none are set.
      if env \
        XDG_DATA_HOME="$data_home" XDG_CACHE_HOME="$cache_home" \
        XDG_CONFIG_HOME="$config_home" XDG_RUNTIME_DIR="$runtime_dir" \
        TMPDIR="$tmp_home" \
        GIO_USE_VFS=local GTK_USE_PORTAL=0 \
        GSK_RENDERER=cairo \
        DISPLAY=":$worker_display" \
        GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
        DISPLAY_TEST="$test" DISPLAY_TEST_PASSED="$display_test_passed" \
        DISPLAY_TEST_OUTPUT="$display_test_output" \
        DISPLAY_TEST_BIN="$test_bin" \
        DISPLAY_TEST_CWD="$repo_root/crates/reprise-gnome" \
        dbus-run-session -- bash -c '
          set -o pipefail
          cd "$DISPLAY_TEST_CWD"
          if ! "$DISPLAY_TEST_BIN" --ignored --exact "$DISPLAY_TEST" \
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
      stop_worker_xvfb
      # An explicit if: under `set -e` a bare `[[ ... ]] && break` would abort
      # the worker on the common case, before the status is ever written.
      if [[ ! -f $display_test_passed ]]; then
        # What the server itself had to say can explain a failed test.
        sed 's/^/xvfb: /' "$marker_dir/xvfb-$attempt.log" || true
      fi
      break
    done
    if [[ -f $display_test_passed ]]; then
      echo pass >"$results_dir/$index.status"
    else
      echo fail >"$results_dir/$index.status"
    fi
  } >"$results_dir/$index.log" 2>&1
  cleanup_worker_roots "$data_home" "$cache_home" "$config_home" \
    "$runtime_dir" "$marker_dir" "$tmp_home"
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

if (( ${#skipped[@]} > 0 )); then
  echo "skipped: ${#skipped[@]} measurement tool(s), not display tests"
  printf '  %s\n' "${skipped[@]}"
fi

if (( ${#failed_tests[@]} > 0 )); then
  printf '  %s\n' "${failed_tests[@]}"
  exit 1
fi
