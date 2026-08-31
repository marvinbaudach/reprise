#!/usr/bin/env python3
"""A real pointer for the takes, through Mutter's own RemoteDesktop API.

Why not ydotool: `ydotoold` here creates a virtual device with `EV=7`
(SYN, KEY, REL) and an empty `abs` capability bitmask — no absolute axes at
all. `ydotool mousemove -a` therefore never positions absolutely; it feeds the
values in as relative deltas, which walked the pointer into the bottom-right
corner and pinned it there. Measured, not deduced: `/proc/bus/input/devices`
and `/sys/class/input/event16/device/capabilities/abs`.

Why not read the position back and correct: under Wayland the X root pointer
only tracks the cursor over XWayland surfaces, so `xdotool getmouselocation`
(and anything else reading X) reports a stale position over native windows.
There is no closed loop to be had that way.

Mutter's RemoteDesktop drives the pointer with no acceleration in the path and
no dependency on X at all. Its absolute call is the obvious one and it is a
trap: `NotifyPointerMotionAbsolute` takes coordinates in a screen-cast stream's
space, and a stream with no PipeWire consumer attached is accepted and then
silently ignored — the call returns cleanly and the pointer does not move. That
cost a full round of "the click fired but the page did not turn".

`NotifyPointerMotionRelative` needs no stream at all. Measured against a
recorded frame, eight large negative deltas pin the pointer in the top-left
corner and a single delta from there lands within the cursor hotspot of the
asked-for point — so relative motion is 1:1 here, and one known origin turns it
into absolute positioning.
"""
import os
import time

from gi.repository import Gio, GLib

BTN_LEFT = 0x110

RD = 'org.gnome.Mutter.RemoteDesktop'
RD_PATH = '/org/gnome/Mutter/RemoteDesktop'
SC = 'org.gnome.Mutter.ScreenCast'
SC_PATH = '/org/gnome/Mutter/ScreenCast'


class Pointer:
    """One RemoteDesktop session, held open for the length of a take.

    The session is bound to the connection that created it, the same way
    `screencast.py` has to hold its own bus connection — dropping it ends the
    session and every later move silently does nothing.
    """

    def __init__(self):
        self.bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
        self.session_path = self._call(RD, RD_PATH, RD, 'CreateSession', None).unpack()[0]
        self._call(RD, self.session_path, RD + '.Session', 'Start', None)
        time.sleep(0.4)
        self.width, self.height = self._logical_screen()
        self.x = self.y = 0.0
        self.home()

    def home(self):
        """Take a fixed reference by driving into a corner the compositor clamps.

        There is no way to read the pointer back: under Wayland the X root
        pointer only tracks the cursor over XWayland surfaces, so xdotool and
        anything else reading X report a stale position over native windows.
        Clamping is the only fixed reference available.

        It has to be the bottom-right corner. Homing into the top-left drove
        the pointer into GNOME's hot corner, which opens the Activities
        overview and takes the focus off the app — the take then aborts one
        step before the click, blaming the move.
        """
        for _ in range(8):
            self._relative(400, 400)
        self.x, self.y = float(self.width), float(self.height)
        time.sleep(0.2)

    def _logical_screen(self):
        r = self._call('org.gnome.Mutter.DisplayConfig',
                       '/org/gnome/Mutter/DisplayConfig',
                       'org.gnome.Mutter.DisplayConfig', 'GetCurrentState', None)
        _, monitors, logical, _ = r.unpack()
        for lm in logical:
            if not lm[4]:
                continue
            scale = lm[2]
            for m in monitors:
                if m[0][0] != lm[5][0][0]:
                    continue
                for md in m[1]:
                    if md[6].get('is-current'):
                        return int(md[1] / scale), int(md[2] / scale)
        raise SystemExit('pointer: no primary logical monitor')

    def _primary_connector(self):
        r = self._call(SC.replace('ScreenCast', 'DisplayConfig'),
                       '/org/gnome/Mutter/DisplayConfig',
                       'org.gnome.Mutter.DisplayConfig', 'GetCurrentState', None)
        _, monitors, logical, _ = r.unpack()
        for lm in logical:
            if lm[4]:  # primary
                return lm[5][0][0]
        return monitors[0][0][0]

    def _get_property(self, path, iface, prop):
        r = self.bus.call_sync(RD, path, 'org.freedesktop.DBus.Properties',
                               'Get', GLib.Variant('(ss)', (iface, prop)),
                               None, Gio.DBusCallFlags.NONE, 5000, None)
        return r.unpack()[0]

    def _call(self, name, path, iface, method, args):
        return self.bus.call_sync(name, path, iface, method, args, None,
                                  Gio.DBusCallFlags.NONE, 10000, None)

    def _relative(self, dx, dy):
        self._call(RD, self.session_path, RD + '.Session',
                   'NotifyPointerMotionRelative',
                   GLib.Variant('(dd)', (float(dx), float(dy))))

    def move_to(self, x, y):
        """Absolute in logical screen coordinates, kept by dead reckoning."""
        self._relative(x - self.x, y - self.y)
        self.x, self.y = float(x), float(y)

    def button(self, pressed, code=BTN_LEFT):
        self._call(RD, self.session_path, RD + '.Session', 'NotifyPointerButton',
                   GLib.Variant('(ib)', (code, bool(pressed))))

    def click(self, hold=0.09):
        self.button(True)
        time.sleep(hold)
        self.button(False)

    def ease_to(self, x0, y0, x1, y1, seconds=0.7, steps=24):
        """Cosine ease, so the pointer starts and stops softly.

        A pointer that jumps is no more legible than no pointer: the whole
        point of putting it back on film is that the viewer sees it arrive.
        """
        import math
        for i in range(1, steps + 1):
            t = i / steps
            s = (1 - math.cos(math.pi * t)) / 2
            self.move_to(x0 + (x1 - x0) * s, y0 + (y1 - y0) * s)
            time.sleep(seconds / steps)

    def close(self):
        try:
            self._call(RD, self.session_path, RD + '.Session', 'Stop', None)
        except Exception:
            pass
