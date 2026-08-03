#!/usr/bin/env bash
#
# PLAY-11 stop-case matrix — the decision cases `filter_clear_playback.sh`
# cannot cover.
#
# `filter_clear_playback.sh` proves the PLAY-11 *hand-off* through the shared
# CUA harness: accessibility exposure plus real input delivery. This runner
# covers the complementary half — every case in which PLAY-11 must NOT hand
# off — and it deliberately does not use AT-SPI at all:
#
#   * each case needs its own library, its own profile and its own app
#     lifecycle, because the decision depends on the origin captured at play
#     time; one shared session cannot express that;
#   * the assertions are about a playback decision, not about a widget, so the
#     app's own diagnostic log is the honest oracle. The screenshots are kept
#     as human-readable evidence, not as the assertion surface.
#
# Input is delivered with xdotool at real screen coordinates and every step is
# photographed with `import`, so a failure can be inspected rather than only
# counted.
#
# Isolation matches AGENTS.md exactly: a private Xvfb display, an own D-Bus
# session, private XDG_{DATA,CACHE,CONFIG}_HOME per case, and `fakesink` audio.
# The user's real database, music and desktop session are never touched.
#
# Usage:
#   scripts/cua-e2e/filter_clear_matrix.sh              # every case
#   scripts/cua-e2e/filter_clear_matrix.sh play-11-stop-filter-active
#
# `CUA_E2E_BIN_PATH` overrides the binary under test; by default the runner
# builds the same target `run.sh` uses.
set -uo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

CUA_E2E_OUT_DIR="${CUA_E2E_OUT_DIR:-/tmp/reprise-play-11-matrix}"
CUA_E2E_SCREEN_RES="${CUA_E2E_SCREEN_RES:-1600x1000x24}"
TRACK_SECONDS="${TRACK_SECONDS:-6}"

# The one log line PLAY-11's hand-off emits (up_next_transport.rs).
MARKER="filtered queue exhausted after filter clear; continuing from random library snapshot"

# Screen coordinates of the controls each case drives, read off the 1600x1000
# layout. They are asserted indirectly: a mis-aimed click shows up as a wrong
# `queue set from view` count, never as a silent pass.
ROW1_X=300; ROW1_Y=326
CLEAR_ALL_X=1332; CLEAR_ALL_Y=236
SEARCH_CHIP_X=472; SEARCH_CHIP_Y=237
REPEAT_X=904; REPEAT_Y=837

ALL_CASES=(
  play-11-continues-after-clear
  play-11-stop-filter-active
  play-11-stop-unfiltered-origin
  play-11-stop-single-title-library
  play-11-stop-playlist-origin
  play-11-stop-facet-still-active
  play-11-stop-repeat-all
)

usage() {
  echo "usage: ${BASH_SOURCE[0]} [case]" >&2
  printf '  %s\n' "${ALL_CASES[@]}" >&2
}

# ---------------------------------------------------------------------------
# Inner run: one case, inside the private display and D-Bus session.
# ---------------------------------------------------------------------------
if [[ "${PLAY11_MATRIX_INNER:-}" == "1" ]]; then
  case_name=$1
  shot() { import -window root "$OUT/$1.png" 2>/dev/null && echo "   [shot] $1.png"; }
  note() { echo "== $*"; }

  # GTK registers the application against `org.a11y.atspi.Registry`. Inside
  # `dbus-run-session` that name is only activatable through systemd, which
  # fails there — GTK then logs a Gtk-CRITICAL that has nothing to do with the
  # product and would poison the clean-log assertion. Run the bus and registry
  # explicitly so the log stays honest.
  /usr/lib/at-spi-bus-launcher --launch-immediately --a11y=1 --screen-reader=1 \
    >"$OUT/at-spi.log" 2>&1 &
  atspi_pid=$!
  sleep 1.5
  a11y_address=$(gdbus call --session -d org.a11y.Bus -o /org/a11y/bus \
    -m org.a11y.Bus.GetAddress 2>&1 | sed -E "s/^\('//; s/',?\)$//")
  export AT_SPI_BUS_ADDRESS="$a11y_address"
  xprop -root -f AT_SPI_BUS 8s -set AT_SPI_BUS "$a11y_address" 2>/dev/null
  /usr/lib/at-spi2-registryd >"$OUT/at-spi-registryd.log" 2>&1 &
  atspi_registry_pid=$!
  sleep 1.2

  fixtures="$SCRATCH/fixtures"
  mkdir -p "$fixtures"
  seed_track() { # seed_track <title> <frequency> <genre> <file>
    ffmpeg -hide_banner -loglevel error -y \
      -f lavfi -i "sine=frequency=$2:duration=$TRACK_SECONDS" \
      -metadata title="$1" -metadata artist="Reprise E2E" -metadata genre="$3" \
      -c:a flac "$fixtures/$4"
  }
  seed_three_track_library() {
    seed_track "Filtered Needle" 440 Needle needle.flac
    seed_track "Library Alpha" 550 Common alpha.flac
    seed_track "Library Beta" 660 Common beta.flac
  }

  declare -a app_env=()
  expect_marker=""; expect_queue_sets=""; description=""
  case "$case_name" in
    play-11-continues-after-clear)
      description="the cleared Music filter hands off to a fresh random library snapshot"
      seed_three_track_library
      app_env=(REPRISE_SMOKE_FILTER=needle)
      expect_marker=yes; expect_queue_sets=2 ;;
    play-11-stop-filter-active)
      description="the search filter is still active when the snapshot ends"
      seed_three_track_library
      app_env=(REPRISE_SMOKE_FILTER=library)
      expect_marker=no; expect_queue_sets=1 ;;
    play-11-stop-unfiltered-origin)
      description="the original snapshot was never filtered"
      seed_three_track_library
      app_env=(REPRISE_SMOKE_ACTIVATE=1)
      expect_marker=no; expect_queue_sets=1 ;;
    play-11-stop-single-title-library)
      description="the library holds no other title to continue with"
      seed_track "Filtered Needle" 440 Needle needle.flac
      app_env=(REPRISE_SMOKE_FILTER=needle)
      expect_marker=no; expect_queue_sets=1 ;;
    play-11-stop-playlist-origin)
      description="the origin is a playlist, not the Music library"
      seed_three_track_library
      app_env=(REPRISE_SMOKE_SEED_PLAYLIST=Mix REPRISE_SMOKE_SOURCE=playlist:1)
      expect_marker=no; expect_queue_sets=1 ;;
    play-11-stop-facet-still-active)
      description="a genre facet survives the search, so the list is not the whole library"
      seed_three_track_library
      app_env=(REPRISE_SMOKE_FILTER=needle REPRISE_SMOKE_BROWSE=genre:Needle)
      expect_marker=no; expect_queue_sets=1 ;;
    play-11-stop-repeat-all)
      description="Repeat All keeps its own queue semantics"
      seed_three_track_library
      app_env=(REPRISE_SMOKE_FILTER=needle)
      expect_marker=no; expect_queue_sets=1 ;;
    *) usage; exit 2 ;;
  esac

  note "$case_name — $description"

  profile="$SCRATCH/profile"
  mkdir -p "$profile/data" "$profile/cache" "$profile/config"
  app_log="$OUT/app.log"
  env "${app_env[@]}" \
    XDG_DATA_HOME="$profile/data" \
    XDG_CACHE_HOME="$profile/cache" \
    XDG_CONFIG_HOME="$profile/config" \
    GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= \
    GTK_A11Y=atspi \
    NO_AT_BRIDGE=0 \
    REPRISE_AUDIO_SINK=fakesink \
    REPRISE_SCAN_DIR="$fixtures" \
    REPRISE_SMOKE_QUIT=1 \
    REPRISE_SMOKE_QUIT_DELAY_SECS=120 \
    REPRISE_LOG=debug \
    "$CUA_E2E_BIN_PATH" >"$app_log" 2>&1 &
  app_pid=$!

  window_id=""
  for _ in $(seq 1 60); do
    window_id=$(xdotool search --class reprise 2>/dev/null | tail -1)
    [[ -n "$window_id" ]] && break
    sleep 0.5
  done
  if [[ -z "$window_id" ]]; then
    echo "$case_name did not expose a Reprise window" >&2
    tail -n 40 "$app_log" >&2 || true
    kill -TERM "$app_pid" 2>/dev/null
    exit 1
  fi
  xdotool windowactivate --sync "$window_id" 2>/dev/null
  sleep 8
  shot 01-initial

  if [[ "$case_name" == "play-11-stop-repeat-all" ]]; then
    # The start-up toast covers the whole transport row; clicking before it
    # expires hits the toast, and the case would pass for the wrong reason.
    note "waiting for the start-up toast to clear the transport row"
    sleep 12
    shot 01a-transport-visible
    note "engaging Repeat All through the transport button"
    xdotool mousemove "$REPEAT_X" "$REPEAT_Y"; sleep 0.3; xdotool click 1
    sleep 1.5
    shot 01b-repeat-engaged
  fi

  if [[ "$case_name" == "play-11-stop-unfiltered-origin" ]]; then
    note "REPRISE_SMOKE_ACTIVATE started row 1 of the unfiltered library"
  else
    note "double-clicking row 1 to start playback"
    xdotool mousemove "$ROW1_X" "$ROW1_Y"; sleep 0.3
    xdotool click --repeat 2 --delay 90 1
    sleep 3
  fi
  shot 02-playing

  case "$case_name" in
    play-11-continues-after-clear|play-11-stop-single-title-library|\
    play-11-stop-playlist-origin|play-11-stop-repeat-all)
      note "clearing the whole filter while playback runs"
      xdotool mousemove "$CLEAR_ALL_X" "$CLEAR_ALL_Y"; sleep 0.3; xdotool click 1
      sleep 2; shot 03-filter-cleared ;;
    play-11-stop-facet-still-active)
      note "removing only the search chip; the genre facet stays"
      xdotool mousemove "$SEARCH_CHIP_X" "$SEARCH_CHIP_Y"; sleep 0.3; xdotool click 1
      sleep 2; shot 03-search-cleared-facet-kept ;;
    *)
      note "leaving the filter untouched — that is this case's point" ;;
  esac

  note "waiting for the snapshot to exhaust"
  observed_marker=no
  for _ in $(seq 1 45); do
    if rg --quiet --fixed-strings "$MARKER" "$app_log"; then
      observed_marker=yes
      break
    fi
    sleep 1
  done
  sleep 3
  shot 04-final

  # `queue set from view` counts snapshot seeds: one for the activation, a
  # second only if PLAY-11 handed off. It catches a mis-aimed click, which a
  # marker check alone would report as a clean stop.
  queue_sets=$(grep -c "queue set from view" "$app_log")
  criticals=$(rg -i -c \
    'Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|BorrowError|BorrowMutError|already borrowed' \
    "$app_log" || echo 0)

  echo
  note "result $case_name"
  echo "   continuation marker : observed=$observed_marker expected=$expect_marker"
  echo "   queue-set-from-view : actual=$queue_sets expected=$expect_queue_sets"
  echo "   critical/panic lines: $criticals (expected 0)"

  status=0
  [[ "$observed_marker" == "$expect_marker" ]] || { echo "   !! marker mismatch"; status=1; }
  [[ "$queue_sets" == "$expect_queue_sets" ]] || { echo "   !! queue-set mismatch"; status=1; }
  [[ "$criticals" == "0" ]] || { echo "   !! log is not clean"; status=1; }
  [[ $status -eq 0 ]] && echo "   => PASS" || echo "   => FAIL"

  kill -TERM "$app_pid" 2>/dev/null; wait "$app_pid" 2>/dev/null
  kill -TERM "$atspi_registry_pid" "$atspi_pid" 2>/dev/null
  exit $status
fi

# ---------------------------------------------------------------------------
# Outer run: prerequisites, the private display, and one child per case.
# ---------------------------------------------------------------------------
case "${1:-}" in
  -h|--help) usage; exit 0 ;;
esac

for command in Xvfb openbox dbus-run-session ffmpeg gdbus import jq rg xdotool xprop; do
  if ! command -v "$command" >/dev/null; then
    echo "required command is unavailable: $command" >&2
    exit 2
  fi
done

if [[ -z "${CUA_E2E_BIN_PATH:-}" ]]; then
  (cd "$repo_root" && cargo build --locked -p reprise-gnome --features test-fixtures) || exit 1
  CUA_E2E_BIN_PATH="$repo_root/target/debug/reprise"
fi
if [[ ! -x "$CUA_E2E_BIN_PATH" ]]; then
  echo "binary under test is missing: $CUA_E2E_BIN_PATH" >&2
  exit 2
fi
export CUA_E2E_BIN_PATH

if [[ -n "${1:-}" ]]; then
  cases=("$1")
else
  cases=("${ALL_CASES[@]}")
fi

mkdir -p "$CUA_E2E_OUT_DIR"
{
  printf 'reprise_commit=%s\n' "$(git -C "$repo_root" rev-parse HEAD)"
  printf 'reprise_tree=%s\n' "$(git -C "$repo_root" rev-parse 'HEAD^{tree}')"
  printf 'binary=%s\n' "$CUA_E2E_BIN_PATH"
  printf 'display_backend=x11-xvfb (private, AT-SPI-free)\n'
} >"$CUA_E2E_OUT_DIR/run-manifest.txt"
echo "[play-11-matrix] evidence: $CUA_E2E_OUT_DIR"

failed=()
for case_name in "${cases[@]}"; do
  echo
  echo "########## $case_name ##########"
  out="$CUA_E2E_OUT_DIR/$case_name"
  rm -rf "$out"; mkdir -p "$out"
  scratch=$(mktemp -d "${TMPDIR:-/tmp}/reprise-play-11-matrix.XXXXXX")

  display_file="$scratch/display"
  Xvfb -displayfd 8 -screen 0 "$CUA_E2E_SCREEN_RES" -nolisten tcp \
    8>"$display_file" >"$out/xvfb.log" 2>&1 &
  xvfb_pid=$!
  for _ in $(seq 1 40); do
    [[ -s "$display_file" ]] && break
    kill -0 "$xvfb_pid" 2>/dev/null || break
    sleep 0.1
  done
  display_number=$(tr -d '[:space:]' <"$display_file")
  if [[ -z "$display_number" ]]; then
    echo "Xvfb did not allocate a private display" >&2
    kill -TERM "$xvfb_pid" 2>/dev/null; rm -rf "$scratch"
    failed+=("$case_name"); continue
  fi
  openbox_display=":$display_number"
  DISPLAY="$openbox_display" openbox >"$out/openbox.log" 2>&1 &
  openbox_pid=$!
  sleep 0.6

  runtime_dir="$scratch/runtime"; mkdir -m 700 "$runtime_dir"
  dbus-run-session -- env \
    -u GNOME_KEYRING_CONTROL -u GNOME_KEYRING_PID \
    DISPLAY="$openbox_display" \
    XDG_RUNTIME_DIR="$runtime_dir" \
    GDK_BACKEND=x11 WAYLAND_DISPLAY= \
    PLAY11_MATRIX_INNER=1 \
    OUT="$out" SCRATCH="$scratch" \
    TRACK_SECONDS="$TRACK_SECONDS" \
    CUA_E2E_BIN_PATH="$CUA_E2E_BIN_PATH" \
    bash "${BASH_SOURCE[0]}" "$case_name"
  [[ $? -eq 0 ]] || failed+=("$case_name")

  kill -TERM "$openbox_pid" "$xvfb_pid" 2>/dev/null
  rm -rf "$scratch"
done

echo
if [[ ${#failed[@]} -eq 0 ]]; then
  echo "[play-11-matrix] all ${#cases[@]} case(s) passed"
  exit 0
fi
echo "[play-11-matrix] failed: ${failed[*]}" >&2
exit 1
