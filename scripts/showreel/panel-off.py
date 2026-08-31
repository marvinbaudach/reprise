#!/usr/bin/env python3
"""Close the info panel and prove it with a frame, not with a property.

Writing `ui.info_panel_visible=0` before the app starts does not close it: the
app came up with the panel open anyway and wrote the key back to `1`. The
toggle is the only thing that moves it, and its `checked` state has already
been read as closed once while the panel stood open in every shot of a take.
So this drives the toggle and then films three seconds, and the answer is the
frame.

    python3 panel-off.py            close it, record proof
    python3 panel-off.py --state    only report what the toggle says
"""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import gi  # noqa: E402
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402

import desk  # noqa: E402
import rp  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
NAME = 'Toggle info panel'


def toggle():
    node = rp.find(NAME)
    if node is None:
        node = rp.find(NAME, exact=False)
    return node


def checked(node):
    try:
        s = node.get_state_set()
        return s.contains(Atspi.StateType.CHECKED)
    except Exception:
        return None


def main():
    state_only = '--state' in sys.argv
    proof_only = '--proof' in sys.argv
    node = toggle()
    if node is None:
        sys.exit(f'ABORT: no {NAME!r} in the accessible tree — is Reprise running?')
    print(f'toggle role={node.get_role_name()} checked={checked(node)} '
          f'actions={rp.actions(node)}', flush=True)
    if state_only:
        return

    if proof_only:
        proof()
        return

    # `do_action` on this toggle returns True and moves nothing — the same
    # reason the tour was rebuilt around a real pointer. Click the button
    # where it actually is.
    from pointer import Pointer
    desk.bring_to_front()
    origin, (fw, fh) = desk.window_origin(desk.active_frame())
    bx, by = desk.centre_of(node, origin)
    print(f'window {fw}x{fh} origin {origin} -> toggle at ({bx:.0f},{by:.0f})',
          flush=True)
    pt = Pointer()
    pt.move_to(bx, by)
    time.sleep(0.6)
    pt.click()
    time.sleep(1.2)
    print(f'after: checked={checked(toggle())}', flush=True)
    if '--film' not in sys.argv:
        return

    proof()


def proof():
    work = rp.work_dir()
    out = os.path.join(work, 'panel-proof.mp4')
    flag = os.path.join(work, 'stop-panel-proof.flag')
    for stale in (out, out + '.mp4', flag):
        if os.path.exists(stale):
            os.remove(stale)

    desk.bring_to_front()
    time.sleep(1.0)
    cast = subprocess.Popen(
        [sys.executable, os.path.join(HERE, 'screencast.py'), out, flag, '20'],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    time.sleep(4.0)
    open(flag, 'w').close()
    cast.wait(timeout=30)
    print(cast.stdout.read().strip(), flush=True)


if __name__ == '__main__':
    main()
