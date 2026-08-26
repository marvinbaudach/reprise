#!/usr/bin/env python3
"""Hold one D-Bus connection for the whole GNOME screencast.

The Screencast session is bound to the connection that started it, so a
second `gdbus call` cannot stop it — the file is then never finalised.
"""
import os
import sys
import time
from gi.repository import Gio, GLib

path = sys.argv[1]
stop_flag = sys.argv[2]
max_seconds = float(sys.argv[3]) if len(sys.argv) > 3 else 900.0
area = sys.argv[4] if len(sys.argv) > 4 else None  # "x,y,w,h"

# The takes were shot with the cursor drawn, and it shows: driving through
# AT-SPI means the pointer never moves, but wherever it happens to be parked it
# sits in frame for the whole take. Off by default now; set SHOWREEL_DRAW_CURSOR
# to reproduce the original footage.
draw_cursor = os.environ.get('SHOWREEL_DRAW_CURSOR', '') not in ('', '0', 'no')

bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
opts = {
    'draw-cursor': GLib.Variant('b', draw_cursor),
    'framerate': GLib.Variant('i', 30),
}
if area:
    x, y, w, h = (int(v) for v in area.split(','))
    method, args = 'ScreencastArea', GLib.Variant('(iiiisa{sv})', (x, y, w, h, path, opts))
else:
    method, args = 'Screencast', GLib.Variant('(sa{sv})', (path, opts))

res = bus.call_sync('org.gnome.Shell.Screencast', '/org/gnome/Shell/Screencast',
                    'org.gnome.Shell.Screencast', method, args,
                    None, Gio.DBusCallFlags.NONE, 10000, None)
ok, real_path = res.unpack()
if not ok:
    print(f'FAILED to start: {real_path}', flush=True)
    sys.exit(1)
print(f'RECORDING {real_path}', flush=True)

deadline = time.time() + max_seconds
while time.time() < deadline and not os.path.exists(stop_flag):
    time.sleep(0.25)

res = bus.call_sync('org.gnome.Shell.Screencast', '/org/gnome/Shell/Screencast',
                    'org.gnome.Shell.Screencast', 'StopScreencast', None,
                    None, Gio.DBusCallFlags.NONE, 10000, None)
print(f'STOPPED {res.unpack()[0]} -> {real_path}', flush=True)
