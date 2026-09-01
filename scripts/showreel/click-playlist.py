#!/usr/bin/env python3
"""Select a sidebar playlist by name, without touching the pointer."""
import sys
import time

sys.path.insert(0, __file__.rsplit('/', 1)[0])
from rp import actions, app_root, do, walk  # noqa: E402

name = sys.argv[1]
frame = app_root().get_child_at_index(0)

# The sidebar rows carry the count beside the name, so the row is matched on a
# prefix rather than on equality.
for node in walk(frame):
    try:
        label = node.get_name() or ''
    except Exception:
        continue
    if not label.startswith(name):
        continue
    available = actions(node)
    for wanted in ('click', 'activate'):
        if wanted in available:
            do(node, wanted)
            print(f'selected {label!r} via {wanted}')
            time.sleep(1.0)
            raise SystemExit(0)

print(f'no clickable row starting with {name!r}', file=sys.stderr)
raise SystemExit(1)
