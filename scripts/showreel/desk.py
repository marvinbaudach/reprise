#!/usr/bin/env python3
"""Reaching the Reprise window, and knowing it is really in front.

Split out of `probe-click.py` once that probe had closed R1, because every
desktop driver from here on needs exactly these four things: the logical screen
size, the active frame, a way to raise the app, and a window-relative rect
turned into a screen coordinate. `probe-click.py` is left as it was — it is the
record of the measurement, not a library.
"""
import os
import subprocess
import time

import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi, Gio  # noqa: E402

import rp  # noqa: E402

KEY_SUPER, KEY_ENTER, KEY_ESC = 125, 28, 1


def logical_screen():
    """Logical (not physical) size of the primary monitor.

    The pointer, AT-SPI and Mutter all speak logical pixels; the mode reports
    physical ones, so every value is divided by the scale.
    """
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    r = bus.call_sync('org.gnome.Mutter.DisplayConfig',
                      '/org/gnome/Mutter/DisplayConfig',
                      'org.gnome.Mutter.DisplayConfig', 'GetCurrentState',
                      None, None, Gio.DBusCallFlags.NONE, 5000, None)
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
    raise SystemExit('ABORT: no primary logical monitor')


def active_frame():
    """The active Reprise frame, or None.

    Walks every frame the app owns rather than trusting child 0: Reprise puts
    progress windows on the bus too, and the first child is not reliably the
    focused one. A real click lands wherever the compositor has the pointer, so
    this is the guard every click is preceded by.
    """
    try:
        app = rp.app_root()
        for i in range(app.get_child_count()):
            f = app.get_child_at_index(i)
            if f is None:
                continue
            if f.get_role_name() != 'frame':
                continue
            if f.get_state_set().contains(Atspi.StateType.ACTIVE):
                return f
    except BaseException:
        pass
    return None


def wait_active(timeout=3.0, poll=0.2):
    """The active frame, waited for instead of sampled once.

    The state blinks. A four-station take found no active frame at its second
    row, between two rows that worked, and the click that guard protects was
    skipped although nothing had taken the focus — a hole in the tour caused by
    the check, not by the desktop. Waiting keeps the guard's promise (never
    click into a window that is not Reprise) without inventing a focus loss out
    of one unlucky reading.
    """
    deadline = time.time() + timeout
    while True:
        frame = active_frame()
        if frame is not None:
            return frame
        if time.time() >= deadline:
            return None
        time.sleep(poll)


def selected_row():
    """Which sidebar row is selected — the app's own answer to "did it turn?".

    Verifying by a widget that "should" be on the new page invents a criterion;
    the selected state is one the app maintains itself.
    """
    for n in rp.walk(rp.app_root()):
        try:
            if n.get_role_name() != 'list item':
                continue
            if n.get_state_set().contains(Atspi.StateType.SELECTED):
                return n.get_name()
        except Exception:
            continue
    return None


def _yd(*args):
    sock = f'/run/user/{os.getuid()}/.ydotool_socket'
    return subprocess.run(['ydotool', *args],
                          env=dict(os.environ, YDOTOOL_SOCKET=sock),
                          capture_output=True, text=True)


def raise_by_search():
    """Super, type the name, Enter — GNOME's own way to reach a window.

    Alt+Tab only cycles the pair of windows GNOME last used, which here is the
    terminal and the browser, so Reprise was never in the rotation. Escape
    first is required: Super toggles, so an overview a previous run left open
    is closed by it and the name is typed into nothing.
    """
    _yd('key', f'{KEY_ESC}:1', f'{KEY_ESC}:0')
    time.sleep(0.8)
    _yd('key', f'{KEY_SUPER}:1', f'{KEY_SUPER}:0')
    time.sleep(2.0)
    _yd('type', 'reprise')
    time.sleep(2.0)
    _yd('key', f'{KEY_ENTER}:1', f'{KEY_ENTER}:0')
    time.sleep(2.5)


def bring_to_front(tries=2, wait=60.0):
    """Raise it, or wait for a person — never proceed blind."""
    for n in range(tries):
        if active_frame() is not None:
            return True
        print(f'Reprise not in front — overview search ({n + 1})', flush=True)
        raise_by_search()
    if active_frame() is not None:
        return True
    print(f'>>> CLICK THE REPRISE WINDOW NOW — waiting up to {wait:.0f}s <<<', flush=True)
    deadline = time.time() + wait
    while time.time() < deadline:
        if active_frame() is not None:
            return True
        time.sleep(0.3)
    return False


def window_origin(frame):
    """Screen origin of a maximized frame, and a check that it is maximized.

    AT-SPI screen coordinates are all zeroes here — a Wayland client does not
    know where it sits — but window-relative ones are correct. For a maximized
    window the origin follows from the logical monitor size: the missing height
    is the top bar.
    """
    fe = frame.get_extents(Atspi.CoordType.WINDOW)
    sw, sh = logical_screen()
    if fe.width != sw:
        raise SystemExit(f'ABORT: window {fe.width} wide vs screen {sw}; need it maximized')
    return (0, sh - fe.height), (fe.width, fe.height)


def centre_of(node, origin):
    """Screen centre of a widget, from its window-relative rect."""
    e = node.get_extents(Atspi.CoordType.WINDOW)
    return origin[0] + e.x + e.width / 2, origin[1] + e.y + e.height / 2


def row_map():
    """Every sidebar row and its window rect, in one reading."""
    out = {}
    for n in rp.walk(rp.app_root()):
        try:
            if n.get_role_name() != 'list item':
                continue
            e = n.get_extents(Atspi.CoordType.WINDOW)
            out[n.get_name()] = (e.x, e.y, e.width, e.height)
        except Exception:
            continue
    return out


def wait_quiescent(reads=3, wait=0.5, tries=40):
    """Hold until the whole sidebar stops moving, not just one row.

    A freshly started Reprise keeps laying out for seconds — the playlist,
    device and issue sections fill in asynchronously and shove the rows around.
    Two runs against a just-launched app clicked rows they never aimed at, and
    the pointer was blameless: it measures accurate to about four pixels over
    exactly those moves.

    Watching a single row was not enough, and that was tried: it agreed with
    itself twice, 0.4 s apart, and moved again afterwards. The list settles in
    steps, so a quiet moment is not the end of the settling. This wants the
    *whole* map unchanged over several consecutive reads before it believes it,
    which is the difference between "nothing moved just now" and "nothing is
    moving any more".
    """
    same = 0
    last = None
    for _ in range(tries):
        cur = row_map()
        if cur and cur == last:
            same += 1
            if same >= reads - 1:
                return cur
        else:
            same = 0
        last = cur
        time.sleep(wait)
    raise SystemExit('ABORT: the sidebar never stopped moving')


def stable_rect(node, tries=12, wait=0.4):
    """One row's rect, taken only once the whole sidebar is quiescent."""
    wait_quiescent()
    return node.get_extents(Atspi.CoordType.WINDOW)


def settled_centre(node, origin):
    """Screen centre of a widget, read only once its rect has settled."""
    e = stable_rect(node)
    return origin[0] + e.x + e.width / 2, origin[1] + e.y + e.height / 2


def settled_row_centre(name, origin, tries=4, wait=1.0):
    """Screen centre of a sidebar row, found by name at the moment of aiming.

    Never hold a row's accessible object across a page turn. The sidebar's
    a11y objects are recreated while the app runs, and a held one raises
    "Object does not exist at path" on the very next read — which is how the
    first tour take died on its third station, after two clicks that worked.
    A name survives what a node reference does not.

    The rebuild also means a row can be *missing* for a moment and be back a
    second later: a resolve pass caught `Music`, `Podcasts` and `YouTube` gone
    while `Radio` and everything below it was there, and all three answered
    again seconds afterwards. So an absent name is retried before it is
    believed. Returns None only if the row stayed away, which the caller must
    treat as a failure rather than aim at a guess.

    `wait_quiescent` already hands back a fresh name -> rect map, so settling
    and looking up are one reading, not two.
    """
    for n in range(tries):
        rects = wait_quiescent()
        e = rects.get(name)
        if e is not None:
            x, y, w, h = e
            return origin[0] + x + w / 2, origin[1] + y + h / 2
        if n + 1 < tries:
            time.sleep(wait)
    return None
