#!/usr/bin/env python3
"""Open the device page, arm the new playlist and start the sync.

The sync page names every playlist as a toggle button, so the row is found by
name and its state is read before it is clicked — a toggle that is already on
would be turned off by a blind click, and the shot would film the opposite of
what it claims.
"""
import sys
import time

sys.path.insert(0, __file__.rsplit('/', 1)[0])

import gi  # noqa: E402
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402
from rp import actions, app_root, do, find, walk  # noqa: E402

name = sys.argv[1]
device = sys.argv[2] if len(sys.argv) > 2 else 'Pixel 10 Pro XL'


def frame():
    return app_root().get_child_at_index(0)


def toggle_for(label):
    for node in walk(frame()):
        try:
            if node.get_role_name() != 'toggle button':
                continue
            if not (node.get_name() or '').startswith(label):
                continue
        except Exception:
            continue
        return node
    return None


opener = find(f'Open {device}', root=frame())
if opener is None:
    raise SystemExit(f'no sidebar entry for {device!r}')
do(opener)
time.sleep(2.5)

row = toggle_for(name)
if row is None:
    raise SystemExit(f'{name!r} is not offered on the sync page')
if row.get_state_set().contains(Atspi.StateType.CHECKED):
    print(f'{name!r} was already armed')
else:
    do(row)
    print(f'armed {name!r}')
time.sleep(2.0)

start = find('Sync now', role='button', root=frame())
if start is None or 'click' not in actions(start):
    raise SystemExit('no usable "Sync now" button')
do(start)
print('sync started')
