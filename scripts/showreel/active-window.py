#!/usr/bin/env python3
"""Print the app name of the window AT-SPI reports as active."""
import gi
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402

desktop = Atspi.get_desktop(0)
for i in range(desktop.get_child_count()):
    app = desktop.get_child_at_index(i)
    if app is None:
        continue
    try:
        for j in range(app.get_child_count()):
            frame = app.get_child_at_index(j)
            if frame is None:
                continue
            states = frame.get_state_set()
            if states.contains(Atspi.StateType.ACTIVE):
                print(f"{app.get_name()} :: {frame.get_name()}")
    except Exception:
        continue
