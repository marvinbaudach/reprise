#!/usr/bin/env bash
# The first-run screen, on a throwaway profile, headless, at the plate's size.
#
# It cannot be shot from the real session: the welcome dialog only appears on
# first run. Xvfb at 3456x2160 with GDK_SCALE=2 gives the same logical 1728x1080
# the real session has, so the window furniture lands at the same size as in the
# other plates. The seeded gtk-4.0/settings.ini is not cosmetic — without the
# decoration layout the window buttons end up on the wrong side.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

REPRISE_BIN="${REPRISE_BIN:-$HOME/.local/bin/reprise}"
P="$SHOWREEL_WORK/welcome"
plate="$SHOWREEL_DIR/welcome-plate.png"

rm -rf -- "$P"
mkdir -p -- "$P/data" "$P/config" "$P/cache" "$P/runtime" "$P/config/gtk-4.0"
chmod 700 -- "$P/runtime"
cat >"$P/config/gtk-4.0/settings.ini" <<'INI'
[Settings]
gtk-decoration-layout=close,minimize:appmenu
gtk-application-prefer-dark-theme=1
INI

Xvfb -displayfd 8 -screen 0 3456x2160x24 8>"$P/display" &
xvfb_pid=$!
sleep 3
disp=":$(cat -- "$P/display")"
printf 'display %s\n' "$disp"
DISPLAY=$disp openbox &
sleep 2

DISPLAY=$disp dbus-run-session -- env \
  XDG_RUNTIME_DIR="$P/runtime" XDG_DATA_HOME="$P/data" \
  XDG_CACHE_HOME="$P/cache" XDG_CONFIG_HOME="$P/config" \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= GDK_SCALE=2 GSK_RENDERER=cairo \
  LIBGL_ALWAYS_SOFTWARE=1 REPRISE_AUDIO_SINK=fakesink \
  "$REPRISE_BIN" >"$P/app.log" 2>&1 &
app_pid=$!

wid=""
for i in $(seq 1 40); do
  wid=$(DISPLAY=$disp xdotool search --class -- reprise 2>/dev/null | head -1)
  [[ -n $wid ]] && break
  sleep 2
done
printf 'window=%s after %sx2s\n' "$wid" "$i"
[[ -n $wid ]] || {
  echo 'no window'
  tail -5 -- "$P/app.log"
  kill "$xvfb_pid" 2>/dev/null || true
  exit 1
}

DISPLAY=$disp wmctrl -i -r "$wid" -b add,maximized_vert,maximized_horz 2>/dev/null || true
sleep 6
DISPLAY=$disp import -window root "$P/welcome-raw.png"
magick "$P/welcome-raw.png" -format "mean=%[mean] colors=%k\n" info:

# The shipped plate is 2400x1456, the same step the other plates are resized to.
# The crop that gets there was not recorded at the time; it is reconstructed
# from the plate's own geometry — 63 rows off the top, which is why its header
# bar is cut. Compare against the shipped plate after a re-shoot.
magick "$P/welcome-raw.png" -crop 3456x2097+0+63 +repage \
  -resize '2400x1456!' "$plate"
printf 'plate -> %s\n' "$plate"

kill "$app_pid" 2>/dev/null || true
sleep 2
kill "$xvfb_pid" 2>/dev/null || true
