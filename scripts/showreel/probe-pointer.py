#!/usr/bin/env python3
"""Prove a real pointer can drive Reprise, and that the film sees it.

The takes were driven through AT-SPI `do_action`, which reaches an element
whatever the pointer is doing — so the pointer never moved and `screencast.py`
switched `draw-cursor` off rather than film a parked arrow. Getting the hand
back means clicking the way a person does: move there, then press.

This probe is the gate on that whole idea. It moves the pointer to a sidebar
button, waits long enough for the target to be legible, clicks with ydotool,
and checks AT-SPI for a widget that only exists on the page it asked for. It
records the run with the cursor drawn, so the answer is visible and not just
asserted.

It never clicks blind. A uinput click lands wherever the compositor has the
pointer, so if Reprise is not the active window the probe aborts — that is the
failure `take-desk.sh` documents for takes 3, 6 and 9, and a real click turns it
from lost footage into a click fired into somebody else's window.
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

HERE = os.path.dirname(os.path.abspath(__file__))
SOCKET = os.environ.get('YDOTOOL_SOCKET', f'/run/user/{os.getuid()}/.ydotool_socket')
# The pointer is eased over this many steps: enough that the move reads as a
# move on film rather than a teleport, few enough that it stays under a second.
STEPS = 24
TRAVEL_S = 0.7
# The hold between arriving and pressing. Without it the arrow and the new page
# appear in the same frame, which is the ghost-hand problem with extra steps.
AIM_S = 0.4


def ydotool(*args):
    env = dict(os.environ, YDOTOOL_SOCKET=SOCKET)
    return subprocess.run(['ydotool', *args], env=env,
                          capture_output=True, text=True)


def move_to(x, y):
    ydotool('mousemove', '-a', '-x', str(int(x)), '-y', str(int(y)))


def ease(x0, y0, x1, y1):
    """Move along a cosine ease so the pointer starts and stops softly."""
    import math
    for i in range(1, STEPS + 1):
        t = i / STEPS
        s = (1 - math.cos(math.pi * t)) / 2
        move_to(x0 + (x1 - x0) * s, y0 + (y1 - y0) * s)
        time.sleep(TRAVEL_S / STEPS)


def logical_screen():
    """The compositor's logical screen size, from Mutter itself."""
    from gi.repository import Gio
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    r = bus.call_sync('org.gnome.Mutter.DisplayConfig',
                      '/org/gnome/Mutter/DisplayConfig',
                      'org.gnome.Mutter.DisplayConfig', 'GetCurrentState',
                      None, None, Gio.DBusCallFlags.NONE, 5000, None)
    _, monitors, logical, _ = r.unpack()
    for lm in logical:
        x, y, scale, _, primary, mons, _ = lm
        if not primary:
            continue
        for m in monitors:
            if m[0][0] != mons[0][0]:
                continue
            for md in m[1]:
                if md[6].get('is-current'):
                    return int(md[1] / scale), int(md[2] / scale)
    raise SystemExit('ABORT: no primary logical monitor')


def window_origin(frame):
    """Where the window sits on screen.

    AT-SPI hands out all-zero SCREEN coordinates here — a Wayland client does
    not know its own position, and every element reports 0,0 with only its size
    filled in. Window-relative coordinates are correct, so the origin is the one
    missing piece, and for a maximized window it follows from the screen: full
    logical width, and whatever is left above it is the top bar.
    """
    e = frame.get_extents(Atspi.CoordType.WINDOW)
    sw, sh = logical_screen()
    if e.width != sw:
        raise SystemExit(f'ABORT: window is {e.width} wide, screen is {sw} — '
                         f'the origin can only be derived while maximized')
    return 0, sh - e.height


def centre(node, origin):
    """Screen centre of a node, from window coordinates plus the origin."""
    e = node.get_extents(Atspi.CoordType.WINDOW)
    ox, oy = origin
    return ox + e.x + e.width / 2, oy + e.y + e.height / 2, e


def active_frame():
    # rp.app_root() raises SystemExit, not Exception, while the app is still
    # coming up — catching only Exception killed the first run of this probe
    # before the window had reached the accessibility bus at all.
    try:
        frame = rp.app_root().get_child_at_index(0)
        return frame if frame.get_state_set().contains(Atspi.StateType.ACTIVE) else None
    except BaseException:
        return None


def on_bus():
    try:
        rp.app_root()
        return True
    except BaseException:
        return False


# Linux keycodes, not X keysyms: ydotool speaks the kernel's language.
KEY_LEFTALT, KEY_TAB = 56, 15


def alt_tab():
    """Raise the other window the way a person would.

    `take-desk.sh` says GNOME 49 on Wayland will not let a script raise Reprise
    back, and takes 3, 6 and 9 filmed a terminal because of it. That is true of
    D-Bus Activate, which Mutter treats as focus stealing. A uinput Alt+Tab is
    not a request to the compositor — it is indistinguishable from the keyboard,
    so the same prevention does not apply.
    """
    ydotool('key', f'{KEY_LEFTALT}:1', f'{KEY_TAB}:1',
            f'{KEY_TAB}:0', f'{KEY_LEFTALT}:0')


def bring_to_front(timeout=30.0):
    deadline = time.time() + timeout
    tries = 0
    while time.time() < deadline:
        if active_frame() is not None:
            return True
        if tries < 4:
            log(f'Reprise is not in front — Alt+Tab ({tries + 1})')
            alt_tab()
            tries += 1
            time.sleep(1.5)
        else:
            time.sleep(0.3)
    return active_frame() is not None


def require_front(where):
    if active_frame() is None:
        sys.exit(f'ABORT: Reprise is not the active window ({where}) — '
                 f'refusing to click blind')


def log(msg):
    print(msg, flush=True)


def main():
    work = rp.work_dir()
    cast = os.path.join(work, 'probe-pointer.mp4')
    flag = os.path.join(work, 'probe-pointer.flag')

    if not os.path.exists(SOCKET):
        sys.exit(f'ABORT: no ydotool socket at {SOCKET}')

    # ---------------------------------------------------------------- the app
    if not subprocess.run(['pgrep', '-f', r'bin/reprise$'],
                          capture_output=True).stdout.strip():
        log('launching Reprise')
        subprocess.Popen([os.path.expanduser('~/.local/bin/reprise')],
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         start_new_session=True)

    log('waiting for Reprise on the accessibility bus')
    deadline = time.time() + 45
    while time.time() < deadline and not on_bus():
        time.sleep(0.3)
    if not on_bus():
        sys.exit('ABORT: Reprise never reached the accessibility bus')
    log('on the bus; bringing it to the front')
    bring_to_front(30.0)
    require_front('before recording')
    log('Reprise is in front')

    # ------------------------------------------------------------ the targets
    frame = rp.app_root().get_child_at_index(0)
    origin = window_origin(frame)
    fe = frame.get_extents(Atspi.CoordType.WINDOW)
    log(f'window {fe.width}x{fe.height} at origin {origin}')

    # The sidebar is list items, not buttons. The first probe asked for a
    # button named Podcasts, got a different widget with an all-zero rect, and
    # drove the pointer into the top-left hot corner.
    target = rp.find('Podcasts', role='list item')
    if target is None:
        sys.exit('ABORT: no "Podcasts" sidebar row on the accessibility bus')
    tx, ty, te = centre(target, origin)
    log(f'target Podcasts -> screen ({tx:.0f},{ty:.0f}) '
        f'from window rect {te.x},{te.y} {te.width}x{te.height}')

    # Start from the far side of the window, so the travel is unmistakable.
    sx = origin[0] + fe.width * 0.72
    sy = origin[1] + fe.height * 0.30

    before = rp.find('Add podcast', role='button') is not None
    log(f'"Add podcast" present before the click: {before}')

    # ----------------------------------------------------------- the recording
    if os.path.exists(flag):
        os.remove(flag)
    env = dict(os.environ, SHOWREEL_DRAW_CURSOR='1')
    rec = subprocess.Popen([sys.executable, os.path.join(HERE, 'screencast.py'),
                            cast, flag, '40'],
                           env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                           text=True)
    time.sleep(3.0)

    # ---------------------------------------------------------------- the move
    require_front('before the move')
    move_to(sx, sy)
    time.sleep(0.8)
    log(f'easing ({sx:.0f},{sy:.0f}) -> ({tx:.0f},{ty:.0f})')
    ease(sx, sy, tx, ty)
    time.sleep(AIM_S)

    require_front('before the click')
    r = ydotool('click', '0xC0')
    log(f'click rc={r.returncode} {r.stdout.strip()} {r.stderr.strip()}')
    time.sleep(2.0)

    # --------------------------------------------------------------- the proof
    after = rp.find('Add podcast', role='button') is not None
    log(f'"Add podcast" present after the click: {after}')

    time.sleep(1.5)
    open(flag, 'w').close()
    try:
        rec.wait(timeout=20)
    except subprocess.TimeoutExpired:
        rec.kill()
    log(rec.stdout.read().strip() if rec.stdout else '')

    verdict = 'PASS' if (after and not before) else 'FAIL'
    log(f'VERDICT {verdict}: the click {"turned" if after else "did not turn"} the page')
    log(f'recording -> {cast}')
    return 0 if verdict == 'PASS' else 1


if __name__ == '__main__':
    sys.exit(main())
