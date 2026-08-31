#!/usr/bin/env python3
"""Does a real pointer click actually drive Reprise? One question, one answer.

No recording here on purpose — the film can only be argued about once the
mechanism works. The proof is a widget that exists on the Podcasts page and
nowhere else: absent before the click, present after.
"""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import gi  # noqa: E402
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402

import rp  # noqa: E402
from pointer import Pointer  # noqa: E402


def logical_screen(bus_owner):
    from gi.repository import Gio
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


def active():
    """The active Reprise frame, or None.

    Looks at every frame the app owns, not just child 0: Reprise puts progress
    windows on the bus too, and the first child is not reliably the one with
    focus.
    """
    try:
        app = rp.app_root()
        for i in range(app.get_child_count()):
            f = app.get_child_at_index(i)
            if f is None:
                continue
            st = f.get_state_set()
            if st.contains(Atspi.StateType.ACTIVE) and f.get_role_name() == 'frame':
                return f
    except BaseException:
        pass
    return None


def selected_row():
    """Which sidebar row is selected.

    The first version of this probe checked for an "Add podcast" button that
    this build does not have, so it reported FAIL for a click that had simply
    not happened yet — a criterion invented rather than looked up. The row's own
    selected state is the app's answer, not mine.
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


def main_frame():
    app = rp.app_root()
    for i in range(app.get_child_count()):
        f = app.get_child_at_index(i)
        if f is not None and f.get_role_name() == 'frame':
            return f
    return None


def yd(*args):
    sock = f'/run/user/{os.getuid()}/.ydotool_socket'
    return subprocess.run(['ydotool', *args],
                          env=dict(os.environ, YDOTOOL_SOCKET=sock),
                          capture_output=True, text=True)


KEY_SUPER, KEY_ENTER, KEY_ESC = 125, 28, 1


def raise_by_search():
    """Super, type the name, Enter — GNOME's own way to reach a window.

    Alt+Tab only cycles the pair of windows GNOME last used, which here is the
    terminal and the browser, so Reprise was never in the rotation. The overview
    search addresses the app by name instead of by history, and it needs no
    pointer and no hand.
    """
    # Escape first: Super toggles, so if the overview is already open — which
    # a previous run can easily have left it — pressing Super closes it and the
    # name is then typed into nothing.
    yd('key', f'{KEY_ESC}:1', f'{KEY_ESC}:0')
    time.sleep(0.8)
    yd('key', f'{KEY_SUPER}:1', f'{KEY_SUPER}:0')
    time.sleep(2.0)
    yd('type', 'reprise')
    time.sleep(2.0)
    yd('key', f'{KEY_ENTER}:1', f'{KEY_ENTER}:0')
    time.sleep(2.5)


def alt_tab():
    """Raise the next window with a real keypress.

    `take-desk.sh` says every Bash call raises the terminal here and GNOME will
    not let a script raise Reprise back — that is true of D-Bus Activate, which
    Mutter treats as focus stealing. ydotool's virtual device has no absolute
    axes, but its keyboard half works, and a uinput keypress is indistinguable
    from the real one.
    """
    sock = f'/run/user/{os.getuid()}/.ydotool_socket'
    subprocess.run(['ydotool', 'key', '56:1', '15:1', '15:0', '56:0'],
                   env=dict(os.environ, YDOTOOL_SOCKET=sock),
                   capture_output=True)


def bring_to_front(tries=2, wait=60.0):
    """Try to raise it; otherwise wait for a person, the way takes already do.

    Alt+Tab through uinput turned out to cycle between whatever pair of windows
    GNOME last used, which need not include Reprise, so it is an attempt and
    not a guarantee. `take-desk.sh` already settled the fallback: the run waits
    inside its pre-roll and the window is clicked forward by hand.
    """
    for n in range(tries):
        if active() is not None:
            return True
        print(f'Reprise not in front — overview search ({n + 1})', flush=True)
        raise_by_search()
    if active() is not None:
        print('Reprise is in front', flush=True)
        return True
    print(f'>>> CLICK THE REPRISE WINDOW NOW — waiting up to {wait:.0f}s <<<', flush=True)
    deadline = time.time() + wait
    while time.time() < deadline:
        if active() is not None:
            print('Reprise is in front', flush=True)
            return True
        time.sleep(0.3)
    return False


def main():
    if not bring_to_front():
        sys.exit('ABORT: could not bring Reprise to the front — refusing to click blind')
    frame = active()
    fe = frame.get_extents(Atspi.CoordType.WINDOW)
    sw, sh = logical_screen(None)
    if fe.width != sw:
        sys.exit(f'ABORT: window {fe.width} wide vs screen {sw}; need it maximized')
    origin = (0, sh - fe.height)
    print(f'window {fe.width}x{fe.height}, screen {sw}x{sh}, origin {origin}', flush=True)

    row = rp.find('Podcasts', role='list item')
    if row is None:
        sys.exit('ABORT: no "Podcasts" sidebar row')
    e = row.get_extents(Atspi.CoordType.WINDOW)
    tx = origin[0] + e.x + e.width / 2
    ty = origin[1] + e.y + e.height / 2
    print(f'target ({tx:.0f},{ty:.0f}) from window rect {e.x},{e.y} {e.width}x{e.height}',
          flush=True)

    before = selected_row()
    print(f'selected row before: {before!r}', flush=True)

    p = Pointer()
    try:
        sx, sy = origin[0] + fe.width * 0.70, origin[1] + fe.height * 0.35
        p.move_to(sx, sy)
        time.sleep(0.6)
        print(f'easing ({sx:.0f},{sy:.0f}) -> ({tx:.0f},{ty:.0f})', flush=True)
        p.ease_to(sx, sy, tx, ty)
        time.sleep(0.4)
        if active() is None:
            sys.exit('ABORT: Reprise lost focus during the move — not clicking')
        p.click()
        print('clicked', flush=True)
        time.sleep(2.0)
    finally:
        p.close()

    after = selected_row()
    print(f'selected row after: {after!r}', flush=True)
    ok = after == 'Podcasts' and before != 'Podcasts'
    print(f'VERDICT {"PASS" if ok else "FAIL"}', flush=True)
    return 0 if ok else 1


if __name__ == '__main__':
    sys.exit(main())
