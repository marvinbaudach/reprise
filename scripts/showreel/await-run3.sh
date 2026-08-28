#!/usr/bin/env bash
# Fire the second pickup take once Reprise is focused.
#
# Same shape as await-run.sh: GNOME 49 on Wayland refuses bring_to_front, so a
# human clicks the window once and this picks it up from there. Its own cast
# and flag names, so it cannot collide with a take-2 run left running.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

here=scripts/showreel
cast="$SHOWREEL_WORK/take3.mp4"
flag="$SHOWREEL_WORK/stop3.flag"

# There is no reliable focus gate on GNOME 49 Wayland, and it is worth writing
# down why rather than shipping a check that only looks like one.
#
#  * active-window.py prints every frame AT-SPI marks ACTIVE, and that is more
#    than one at a time — a browser stays "active" while the app in front of it
#    is the one being used. Matching '^reprise' fires while something else is
#    on top; demanding it be the only match never fires at all.
#  * org.freedesktop.Application.Activate over D-Bus returns success and does
#    not raise the window. Measured 26.08.2026: the call returns (), a probe
#    screencast two seconds later still shows the browser.
#
# What does work is looking. Record two seconds, pull a frame, and see what is
# actually there — the same evidence the take itself will record. That needs a
# pair of eyes, so this waits for the loose signal and the operator confirms.
hit=0
for i in $(seq 1 600); do
  if python3 "$here/active-window.py" 2>/dev/null | grep -qi '^reprise'; then
    hit=1
    break
  fi
  sleep 2
done

[[ $hit == 1 ]] || {
  echo TIMEOUT
  exit 1
}
printf 'focus after %s polls\n' "$i"

rm -f -- "$flag" "$cast"
python3 "$here/screencast.py" "$cast" "$flag" 200 >"$SHOWREEL_WORK/cast-take3.log" 2>&1 &
sleep 2
set +e
python3 "$here/take-gnome3.py" >"$SHOWREEL_WORK/take3.log" 2>&1
rc=$?
set -e
touch -- "$flag"
sleep 3
printf 'take3 rc=%s  %s\n' "$rc" "$(showreel_duration "$cast" 2>/dev/null || echo 'no file')"
