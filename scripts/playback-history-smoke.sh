#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
mode="${1:-}"
if [[ "$mode" != "--expect-broken" && -n "$mode" ]]; then
  printf 'usage: %s [--expect-broken]\n' "$0" >&2
  exit 2
fi

out="${PLAYBACK_HISTORY_OUT_DIR:-$(mktemp -d)}"
mkdir -p "$out"
fixture="$(mktemp -d)"
trap 'rm -rf "$fixture"' EXIT

for number in 1 2 3 4 5; do
  ffmpeg -nostdin -loglevel error -y \
    -f lavfi -i 'anullsrc=r=44100:cl=stereo' -t 30 \
    -metadata "title=History ${number}" \
    -metadata 'artist=Playback History Smoke' \
    -metadata 'album=Isolated Fixture' \
    -c:a flac "$fixture/history-${number}.flac"
done

now_title() {
  busctl --user get-property "$BUS" /org/mpris/MediaPlayer2 \
    org.mpris.MediaPlayer2.Player Metadata \
    | sed -n 's/.*"xesam:title" s "\([^"]*\)".*/\1/p'
}

position_ms() {
  busctl --user get-property "$BUS" /org/mpris/MediaPlayer2 \
    org.mpris.MediaPlayer2.Player Position \
    | awk '{ print int($2 / 1000) }'
}

starts_since() {
  local mark="$1"
  awk -v mark="$mark" 'NR > mark && /playback started/' "$OUT/app.log" | wc -l
}

wait_for_bus() {
  local attempt
  # A cold worktree may spend several minutes compiling before the process can
  # own its bus name. Poll the bus throughout; app.log distinguishes that
  # normal build from a process that exited.
  for ((attempt = 1; attempt <= 1500; attempt++)); do
    if ! kill -0 "$APP_PID" 2>/dev/null; then
      printf 'FAIL application exited before owning MPRIS; see %s\n' "$OUT/app.log"
      return 1
    fi
    if busctl --user list 2>/dev/null | awk '{ print $1 }' | grep -Fqx "$BUS"; then
      return 0
    fi
    sleep 0.2
  done
  printf 'FAIL MPRIS bus %s did not appear\n' "$BUS"
  return 1
}

wait_for_title() {
  local expected="${1:-}"
  local rejected="${2:-}"
  local attempt title
  for attempt in {1..200}; do
    title="$(now_title || true)"
    if [[ -n "$title" && ( -z "$expected" || "$title" == "$expected" ) \
          && ( -z "$rejected" || "$title" != "$rejected" ) ]]; then
      printf '%s\n' "$title"
      return 0
    fi
    sleep 0.1
  done
  printf 'FAIL title did not settle (expected=%s rejected=%s current=%s)\n' \
    "$expected" "$rejected" "$(now_title || true)" >&2
  return 1
}

wait_for_starts() {
  local mark="$1"
  local expected="$2"
  local attempt count
  for attempt in {1..200}; do
    count="$(starts_since "$mark")"
    if (( count >= expected )); then
      return 0
    fi
    sleep 0.1
  done
  printf 'FAIL playback starts did not reach %s (got %s)\n' \
    "$expected" "$(starts_since "$mark")" >&2
  return 1
}

wait_for_position_at_most() {
  local limit="$1"
  local attempt current
  for attempt in {1..100}; do
    current="$(position_ms)"
    if (( current <= limit )); then
      return 0
    fi
    sleep 0.1
  done
  printf 'FAIL position did not fall to at most %s ms (got %s)\n' \
    "$limit" "$(position_ms)" >&2
  return 1
}

wait_for_position_above() {
  local limit="$1"
  local attempt current
  for attempt in {1..200}; do
    current="$(position_ms)"
    if (( current > limit )); then
      return 0
    fi
    sleep 0.1
  done
  printf 'FAIL position did not rise above %s ms (got %s)\n' \
    "$limit" "$(position_ms)" >&2
  return 1
}

wait_for_shuffle() {
  local attempt value
  for attempt in {1..100}; do
    value="$(busctl --user get-property "$BUS" /org/mpris/MediaPlayer2 \
      org.mpris.MediaPlayer2.Player Shuffle 2>/dev/null || true)"
    if [[ "$value" == 'b true' || "$value" == 'b 1' ]]; then
      return 0
    fi
    sleep 0.1
  done
  printf 'FAIL Shuffle did not become true (readback=%s)\n' "$value"
  return 1
}

transport() {
  busctl --user call "$BUS" /org/mpris/MediaPlayer2 \
    org.mpris.MediaPlayer2.Player "$1" >/dev/null
}

report_step() {
  local label="$1"
  local mark="$2"
  local title position starts
  title="$(now_title)"
  position="$(position_ms)"
  starts="$(starts_since "$mark")"
  printf 'STEP %-22s title=%-12s position_ms=%-6s starts=%s\n' \
    "$label" "$title" "$position" "$starts" | tee -a "$OUT/report.txt"
}

expect_broken() {
  local first next mark after_back after_dead_one after_dead_two
  first="$(wait_for_title)"
  busctl --user set-property "$BUS" /org/mpris/MediaPlayer2 \
    org.mpris.MediaPlayer2.Player Shuffle b true
  wait_for_shuffle

  transport Next
  next="$(wait_for_title '' "$first")"
  mark="$(wc -l < "$OUT/app.log")"
  report_step 'heard-next' "$mark"

  transport Previous
  wait_for_title "$first" >/dev/null
  wait_for_starts "$mark" 1
  after_back="$(now_title)"
  report_step 'first-previous' "$mark"

  transport Previous
  wait_for_starts "$mark" 2
  after_dead_one="$(now_title)"
  report_step 'second-previous' "$mark"

  transport Previous
  wait_for_starts "$mark" 3
  after_dead_two="$(now_title)"
  report_step 'third-previous' "$mark"

  if [[ "$first" != "$next" && "$after_back" == "$first" \
        && "$after_dead_one" == "$first" && "$after_dead_two" == "$first" ]]; then
    printf 'RESULT broken behavior reproduced: one back step, then repeated restarts\n' \
      | tee -a "$OUT/report.txt"
    return 0
  fi
  printf 'FAIL baseline did not reproduce the expected dead Previous behavior\n'
  return 1
}

expect_fixed() {
  local first second current mark rewound
  local -a heard back forward
  first="$(wait_for_title)"
  busctl --user set-property "$BUS" /org/mpris/MediaPlayer2 \
    org.mpris.MediaPlayer2.Player Shuffle b true
  wait_for_shuffle

  # Step 1: an early Previous on the first entry is a seek, not a restart.
  mark="$(wc -l < "$OUT/app.log")"
  transport Previous
  wait_for_title "$first" >/dev/null
  wait_for_position_at_most 750
  report_step '1-rewind-first' "$mark"
  if [[ "$(starts_since "$mark")" != 0 ]]; then
    printf 'FAIL step 1 restarted the initial track\n'
    return 1
  fi

  # Step 2: Next and Previous return to the actual first entry.
  mark="$(wc -l < "$OUT/app.log")"
  transport Next
  second="$(wait_for_title '' "$first")"
  wait_for_starts "$mark" 1
  transport Previous
  wait_for_title "$first" >/dev/null
  wait_for_starts "$mark" 2
  report_step '2-next-then-back' "$mark"
  if [[ "$(starts_since "$mark")" != 2 ]]; then
    printf 'FAIL step 2 did not make exactly two real track changes\n'
    return 1
  fi

  # Step 3: exhausted history rewinds and creates no new pipeline start.
  mark="$(wc -l < "$OUT/app.log")"
  transport Previous
  wait_for_title "$first" >/dev/null
  wait_for_position_at_most 750
  report_step '3-exhausted-rewind' "$mark"
  if [[ "$(starts_since "$mark")" != 0 ]]; then
    printf 'FAIL step 3 restarted at exhausted history\n'
    return 1
  fi

  # Step 4: hear three advances, then walk those prior entries backwards.
  heard=("$first")
  current="$first"
  mark="$(wc -l < "$OUT/app.log")"
  for expected_starts in 1 2 3; do
    transport Next
    current="$(wait_for_title '' "$current")"
    wait_for_starts "$mark" "$expected_starts"
    heard+=("$current")
  done
  back=()
  for index in 2 1 0; do
    transport Previous
    wait_for_title "${heard[$index]}" >/dev/null
    back+=("$(now_title)")
  done
  wait_for_starts "$mark" 6
  report_step '4-three-back' "$mark"
  if [[ "${back[*]}" != "${heard[2]} ${heard[1]} ${heard[0]}" ]]; then
    printf 'FAIL step 4 heard=%s back=%s\n' "${heard[*]}" "${back[*]}"
    return 1
  fi

  # Step 5: the same branch is available in its original direction.
  forward=()
  mark="$(wc -l < "$OUT/app.log")"
  for index in 1 2 3; do
    transport Next
    wait_for_title "${heard[$index]}" >/dev/null
    forward+=("$(now_title)")
  done
  wait_for_starts "$mark" 3
  report_step '5-three-forward' "$mark"
  if [[ "${forward[*]}" != "${heard[1]} ${heard[2]} ${heard[3]}" ]]; then
    printf 'FAIL step 5 heard=%s forward=%s\n' "${heard[*]}" "${forward[*]}"
    return 1
  fi

  # Step 6: after three seconds the first press rewinds, the second goes back.
  wait_for_position_above 3000
  mark="$(wc -l < "$OUT/app.log")"
  transport Previous
  wait_for_title "${heard[3]}" >/dev/null
  wait_for_position_at_most 750
  rewound="$(now_title)"
  if [[ "$(starts_since "$mark")" != 0 ]]; then
    printf 'FAIL step 6 rewind restarted the pipeline\n'
    return 1
  fi
  transport Previous
  wait_for_title "${heard[2]}" >/dev/null
  wait_for_starts "$mark" 1
  report_step '6-rewind-then-back' "$mark"
  if [[ "$rewound" != "${heard[3]}" || "$(now_title)" != "${heard[2]}" \
        || "$(starts_since "$mark")" != 1 ]]; then
    printf 'FAIL step 6 did not rewind once and change once\n'
    return 1
  fi

  printf 'RESULT all six PLAY-14 MPRIS steps passed (heard=%s)\n' \
    "${heard[*]}" | tee -a "$OUT/report.txt"
}

session_main() {
  set -euo pipefail
  local requested_mode="$1"
  export FIXTURE="$2"
  export OUT="$3"
  export REPO_ROOT="$4"
  export BUS='org.mpris.MediaPlayer2.reprise.PlaybackHistorySmoke'
  : > "$OUT/app.log"
  : > "$OUT/report.txt"

  setsid cargo run --manifest-path "$REPO_ROOT/Cargo.toml" -p reprise-gnome \
    > "$OUT/app.log" 2>&1 &
  local app_pid=$!
  export APP_PID="$app_pid"
  cleanup_app() {
    kill -- "-$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
  }
  trap cleanup_app EXIT

  wait_for_bus
  if [[ "$requested_mode" == '--expect-broken' ]]; then
    expect_broken
  else
    expect_fixed
  fi
}

export -f now_title position_ms starts_since wait_for_bus wait_for_title
export -f wait_for_starts wait_for_position_at_most wait_for_position_above
export -f wait_for_shuffle transport report_step expect_broken expect_fixed
export -f session_main

printf 'OUTPUT_DIR=%s\n' "$out"

# Keep this complete isolation command in one place. The application inherits
# a private session bus, X11 server, XDG roots and fake audio sink; the fixture
# is generated above and never points at the user's library.
timeout 420s dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME="$(mktemp -d)" XDG_CACHE_HOME="$(mktemp -d)" \
  XDG_CONFIG_HOME="$(mktemp -d)" XDG_RUNTIME_DIR="$(mktemp -d)" \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  GIO_USE_VFS=local GTK_USE_PORTAL=0 NO_AT_BRIDGE=1 GTK_A11Y=none REPRISE_LOG=info \
  REPRISE_SCAN_DIR="$fixture" REPRISE_SMOKE_ACTIVATE=1 \
  REPRISE_SMOKE_MPRIS_BUS_NAME='org.mpris.MediaPlayer2.reprise.PlaybackHistorySmoke' \
  bash -c 'session_main "$@"' playback-history-session \
  "$mode" "$fixture" "$out" "$repo_root"
