#!/usr/bin/env python3
"""Block until the Reprise window is the active one, or give up.

Every Bash call raises the terminal on this desktop and GNOME 49 on Wayland
will not let a script raise Reprise back, so a take starts with the window
clicked forward by hand. Takes 3, 6, 9 and the first MCP take all died because
nothing checked whether that click had happened — they recorded a terminal and
only the frames showed it. This is the check: no active window, no recording.
"""
import sys
import time

sys.path.insert(0, __file__.rsplit('/', 1)[0])

import gi  # noqa: E402
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402
from rp import app_root  # noqa: E402

deadline = time.time() + (float(sys.argv[1]) if len(sys.argv) > 1 else 30.0)
while time.time() < deadline:
    try:
        frame = app_root().get_child_at_index(0)
        if frame.get_state_set().contains(Atspi.StateType.ACTIVE):
            print('Reprise is in front')
            raise SystemExit(0)
    except SystemExit as done:
        if done.code == 0:
            raise
    except Exception:
        pass
    time.sleep(0.25)

print('Reprise never came to the front — take aborted', file=sys.stderr)
raise SystemExit(1)
