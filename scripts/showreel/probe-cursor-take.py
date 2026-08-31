#!/usr/bin/env python3
"""Does a take contain a moving hand? Record and drive at once, then prove it.

R1 was closed in two halves that were never joined: `probe-click.py` showed a
real pointer drives the app, and a separate still showed `SHOWREEL_DRAW_CURSOR`
puts the cursor in the recording. This runs both at once and leaves a file that
can be read frame by frame — which is the only way a shot has ever been
verified here.

Deliberately small: three sidebar rows, one click each, ~20 s. It is a proof,
not a take. Must be started detached, or the Bash call raises the terminal and
the guard aborts the run.
"""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import desk  # noqa: E402
import rp  # noqa: E402
from pointer import Pointer  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
ROWS = ('Podcasts', 'Radio', 'Concerts')
DWELL = 1.6


def start_cast(path, flag, budget=90):
    """GNOME appends its own extension, so the asked-for path is not the
    written one. The real path comes back on the RECORDING line."""
    env = dict(os.environ, SHOWREEL_DRAW_CURSOR='1')
    p = subprocess.Popen([sys.executable, os.path.join(HERE, 'screencast.py'),
                          path, flag, str(budget)],
                         stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                         text=True, env=env)
    line = p.stdout.readline().strip()
    if not line.startswith('RECORDING '):
        p.kill()
        raise SystemExit(f'ABORT: screencast did not start: {line!r}')
    return p, line[len('RECORDING '):]


def main():
    work = rp.work_dir()
    asked = os.path.join(work, 'probe-cursor.mp4')
    flag = os.path.join(work, 'stop-probe-cursor.flag')
    for stale in (asked, asked + '.mp4', flag):
        if os.path.exists(stale):
            os.remove(stale)

    if not desk.bring_to_front():
        sys.exit('ABORT: could not bring Reprise to the front — refusing to click blind')
    frame = desk.active_frame()
    origin, (fw, fh) = desk.window_origin(frame)
    desk.wait_quiescent()
    print(f'window {fw}x{fh}, origin {origin}', flush=True)

    targets = []
    for name in ROWS:
        node = rp.find(name, role='list item')
        if node is None:
            sys.exit(f'ABORT: no "{name}" sidebar row')
        targets.append((name, node))

    cast, real_path = start_cast(asked, flag)
    print(f'recording -> {real_path}', flush=True)
    time.sleep(2.5)

    marks, failures = [], []
    p = Pointer()
    t0 = time.time()
    try:
        x, y = origin[0] + fw * 0.70, origin[1] + fh * 0.35
        p.move_to(x, y)
        time.sleep(0.8)
        for name, node in targets:
            # Re-read immediately before aiming, not once up front: the rect is
            # only trustworthy at the moment it is used.
            for attempt in (1, 2):
                tx, ty = desk.settled_centre(node, origin)
                before = desk.selected_row()
                marks.append((round(time.time() - t0, 2), name, (round(tx), round(ty))))
                p.ease_to(x, y, tx, ty, seconds=1.0)
                x, y = tx, ty
                time.sleep(0.4)
                if desk.active_frame() is None:
                    failures.append(f'{name}: lost focus before the click')
                    break
                p.click()
                time.sleep(DWELL)
                after = desk.selected_row()
                print(f'{name}: {before!r} -> {after!r}'
                      + ('' if attempt == 1 else '  (retry)'), flush=True)
                if after == name:
                    break
                # A miss is worth one retry, because the cause is a rect that
                # went stale between reading and clicking — reading it again is
                # exactly the cure. A second miss is a different animal and
                # must not be papered over.
                if attempt == 2:
                    failures.append(f'{name}: row did not select (got {after!r})')
    finally:
        p.close()
        time.sleep(1.0)
        open(flag, 'w').close()
        cast.wait(timeout=30)

    print('MARKS ' + '; '.join(f'{t}s {n} at {c}' for t, n, c in marks), flush=True)
    print(f'FILM {real_path}', flush=True)
    if failures:
        for f in failures:
            print(f'FAIL {f}', flush=True)
        return 1
    print('VERDICT drive PASS — now read the film for the cursor', flush=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
