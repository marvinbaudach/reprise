#!/usr/bin/env bash
# Fire the pickup take once Reprise is focused.
#
# The window has to be clicked once by a human: GNOME 49 on Wayland refuses
# bring_to_front, and a take driven at an unfocused window records the wrong
# surface. This waits for that click in the background and runs the take, the
# stop of the screencast and the plates without a second hand on the keyboard.
#
# The focus check must not run twice: the window can lose focus again between
# the loop and a guard, which then reads as a timeout on a take that was fine.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

here=scripts/showreel
cast="$SHOWREEL_WORK/take2.mp4"
flag="$SHOWREEL_WORK/stop2.flag"

hit=0
for i in $(seq 1 1500); do
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

rm -f -- "$flag" "$cast.mp4"
python3 "$here/screencast.py" "$cast" "$flag" 200 >"$SHOWREEL_WORK/cast-take2.log" 2>&1 &
sleep 2
python3 "$here/take-gnome2.py" >"$SHOWREEL_WORK/take2.log" 2>&1
echo "take2 rc=$?"
touch -- "$flag"
sleep 3
python3 "$here/plates-gnome.py" >"$SHOWREEL_WORK/plates.log" 2>&1
echo "plates rc=$?"

shopt -s nullglob
plates=0
for shot in "$SHOWREEL_WORK"/plates/*.png; do
  [[ $shot == *-raw.png ]] && continue
  plates=$((plates + 1))
done
printf '%d plate(s)\n' "$plates"
