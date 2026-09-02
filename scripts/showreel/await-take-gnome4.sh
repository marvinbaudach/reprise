#!/usr/bin/env bash
# Wait for a human to click Reprise, then fire take-gnome4.py.
#
# GNOME 49 on Wayland refuses to raise a window from outside it: D-Bus
# `Activate` returns success and does nothing, and `desk.raise_by_search()` —
# the overview route the take uses itself — fails often enough that a take
# aborted on 2026-09-02 having spent nothing but a minute. So a person clicks
# the window once and this picks it up from there.
#
# Why the pause after the click matters, and why it is this long. A take whose
# first pointer move lands seconds after a human click retries its first
# stations, and a retry is what a lost take looks like from the outside: the
# 2026-08-29 third take retried Podcasts and YouTube and ended FAIL, the
# documented signature of focus being stolen. Six seconds is cheap insurance.
#
# Run it detached, in its own unit — a process started from an agent's shell is
# reaped 60-90 s after the spawning call returns, and this one waits minutes:
#
#   systemd-run --user --unit=showreel-await --same-dir \
#     --setenv=WAYLAND_DISPLAY="$WAYLAND_DISPLAY" --setenv=DISPLAY="$DISPLAY" \
#     --setenv=XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" --setenv=XDG_CURRENT_DESKTOP=GNOME \
#     scripts/showreel/await-take-gnome4.sh --limit 7
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

LOG="$SHOWREEL_WORK/take.log"
SETTLE="${SHOWREEL_SETTLE:-6}"
WAIT_TICKS="${SHOWREEL_WAIT_TICKS:-300}"   # 2 s each, so ten minutes

# active-window.py prints every window AT-SPI marks ACTIVE, and that is more
# than one at a time — a browser stays "active" behind the app in front of it.
# So this is a loose signal, not a proof: it says Reprise is among the active
# windows, and the take's own focus guard is what decides whether to shoot.
printf 'await: click the Reprise window; waiting up to %d s\n' "$((WAIT_TICKS * 2))" >&2
for _ in $(seq 1 "$WAIT_TICKS"); do
  if python3 scripts/showreel/active-window.py 2>/dev/null | grep -qi '^reprise'; then
    printf 'await: Reprise is active; settling %s s before the first move\n' "$SETTLE" >&2
    sleep "$SETTLE"
    exec python3 scripts/showreel/take-gnome4.py "$@" >"$LOG" 2>&1
  fi
  sleep 2
done

printf 'await: TIMEOUT — Reprise never became active\n' >&2
exit 1
