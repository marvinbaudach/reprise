#!/usr/bin/env bash
# One desktop take: pre-roll, screencast, driver, stop.
#
# Every Bash call raises the terminal on this desktop, and GNOME 49 on Wayland
# will not let a script raise Reprise back — takes 3, 6 and 9 of the earlier
# shoot died exactly that way, filming a terminal instead of an app. So this
# script does the waiting itself: it must be started detached, the window is
# clicked back by hand inside the pre-roll, and nothing may be run in the shell
# until it prints its last line.
#
# It writes to a new file rather than over roh-gnome-take1/2.mp4. Those are the
# only footage that exists until this one is checked.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

driver=${1:?driver script}
name=${2:?take name}
budget=${3:-300}

here=scripts/showreel
cast="$SHOWREEL_DIR/roh-$name.mp4"
flag="$SHOWREEL_WORK/stop-$name.flag"

PREROLL="${SHOWREEL_PREROLL:-16}"
printf 'click Reprise now — recording starts once it is in front (up to %ss)\n' "$PREROLL"
# Not a blind sleep. The first MCP take and takes 3, 6 and 9 recorded a terminal
# because nothing checked whether the click had landed, and only the frames told
# us. wait-active.py fails the take instead of filming the wrong window.
python3 "$here/wait-active.py" "$PREROLL"

rm -f -- "$flag"
python3 "$here/screencast.py" "$cast" "$flag" "$budget" >"$SHOWREEL_WORK/cast-$name.log" 2>&1 &
sleep 3

python3 "$here/$driver" >"$SHOWREEL_WORK/$name.log" 2>&1 || printf 'driver exited %s\n' "$?"

sleep 2
touch -- "$flag"
sleep 4
printf 'take %s -> %s\n' "$name" "$cast"
