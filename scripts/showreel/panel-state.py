#!/usr/bin/env python3
"""Is the info panel open? Ask the window, not the toggle.

The toggle's CHECKED state is never set — it reads False with the panel plainly
open — and there is no cheap picture here: the Screenshot portal refuses, and a
screencast flashes the desk for a question this small.

The reading is the panel's own cover: an open panel puts a large square in the
window's right-hand column, a closed one puts nothing that big there. Counting
*all* widgets right of the column, as this script used to, does not survive the
closed case — with the panel gone the track list widens to the right edge and
its own columns land in the count. Measured 2026-08-31: the old rule read 103
widgets with the panel demonstrably closed and called it OPEN.

Control arms, same window (1728 logical px, maximised):

    open    several panels >=100x100 at x=1474, incl. the 170x170 cover
    closed  zero widgets >=100x100 right of x=1400

    python3 panel-state.py        OPEN or CLOSED, with the evidence
"""
import sys
import os

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import gi  # noqa: E402
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402

import rp  # noqa: E402

# The window is 1728 logical px wide and the panel is the last ~250 of it.
RIGHT = 1400
# The cover is ~170x170. Nothing in the closed layout reaches this size here.
MIN_SIDE = 100


def big_widgets():
    root = rp.app_root()
    if root is None:
        sys.exit('ABORT: Reprise not on the accessibility bus')
    out = []
    for n in rp.walk(root, limit=400):
        try:
            e = n.get_extents(Atspi.CoordType.WINDOW)
            ss = n.get_state_set()
        except Exception:
            continue
        if e.x >= RIGHT and e.width >= MIN_SIDE and e.height >= MIN_SIDE \
                and ss.contains(Atspi.StateType.SHOWING):
            out.append((n.get_role_name(), (n.get_name() or '')[:28],
                        (e.x, e.y, e.width, e.height)))
    return out


if __name__ == '__main__':
    c = big_widgets()
    print(f'{"OPEN" if c else "CLOSED"}  '
          f'({len(c)} widgets >={MIN_SIDE}px square right of x={RIGHT})')
    for r in c[:6]:
        print('   ', r)
