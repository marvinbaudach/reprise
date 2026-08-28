#!/usr/bin/env bash
# The MCP take: an agent builds a playlist, and the running app shows it arrive.
#
# No terminal is filmed. The whole point of the feature is that the library
# changes underneath a window nobody touched, so the window is all there is to
# see — the request becomes a caption in the cut, not a shell on screen.
#
# One launch, then hands off. Every Bash call raises the terminal on this
# desktop, and takes 3, 6 and 9 of the earlier shoot died exactly that way, so
# this script does the waiting itself and must be started detached.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

here=scripts/showreel
cast="$SHOWREEL_WORK/take-mcp.mp4"
flag="$SHOWREEL_WORK/stop-mcp.flag"
DB="${REPRISE_DB:-$HOME/.local/share/reprise/reprise.db}"
# Built from origin/dev, not from this branch. The live database is at schema
# 80 and this branch's core still supports 79, so the branch binary refuses to
# open it — correctly. The shot needs the server that matches the running app.
MCP="${REPRISE_MCP:-.worktrees/mcp-dev/target/debug/reprise-mcp}"
NAME="${MCP_PLAYLIST_NAME:-Like Lorna Shore}"

# Launching this raises the terminal, and GNOME 49 on Wayland will not let a
# script put Reprise back on top. So the recording does not start with the
# script: it waits, and the window is clicked back into place by hand first.
PREROLL="${SHOWREEL_PREROLL:-16}"
printf 'click Reprise now — recording starts once it is in front (up to %ss)\n' "$PREROLL"
# Not a blind sleep. The first MCP take and takes 3, 6 and 9 recorded a terminal
# because nothing checked whether the click had landed, and only the frames told
# us. wait-active.py fails the take instead of filming the wrong window.
python3 "$here/wait-active.py" "$PREROLL"

rm -f -- "$flag"
python3 "$here/screencast.py" "$cast" "$flag" 90 >"$SHOWREEL_WORK/cast-mcp.log" 2>&1 &
sleep 3

# The library, untouched, before anything happens. This is the stretch the cut
# blurs and lays the request over.
sleep 4

python3 "$here/mcp-playlist.py" "$MCP" "$DB" "$NAME" >"$SHOWREEL_WORK/mcp.json" 2>"$SHOWREEL_WORK/mcp.log"

# The desktop watches the database directory and debounces 250 ms before it
# re-reads, so the sidebar row appears a moment after the write, not with it.
# Filming that wait is the point: it is the feature.
sleep 6

python3 "$here/click-playlist.py" "$NAME" >>"$SHOWREEL_WORK/mcp.log" 2>&1 || true
sleep 5

# And then the other half of the story: the playlist the agent wrote is armed
# on the device page and pushed to the phone. It is the same object travelling
# the whole way — written by a tool, chosen in the UI, carried to the handset.
python3 "$here/sync-playlist.py" "$NAME" >>"$SHOWREEL_WORK/mcp.log" 2>&1 || true
sleep 9

touch -- "$flag"
sleep 3
printf 'take-mcp -> %s\n' "$cast"
