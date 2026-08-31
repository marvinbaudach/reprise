#!/usr/bin/env python3
"""The MCP take, with a hand in it.

The old take (`take-mcp.sh`, 2026-08-28) is right about the shot and wrong
about one thing: it selects the new playlist through AT-SPI, so the row opens
with nothing on screen causing it — the same defect the whole desktop tour was
re-shot to remove. Here the request still arrives without a pointer, because
that *is* the claim (the library changes underneath a window nobody touched),
and only the last beat has a hand: a visible cursor walks to the row that just
appeared and opens it.

The seed is `Bring Me The Horizon`, not `Lorna Shore`. The Lorna Shore playlist
already exists in this library and is being carried to the phone in the sync
shot; a second row reading almost the same would look like a duplicate rather
than like an answer.

Run it detached — a foreground Bash call raises the terminal and the focus
guard aborts rather than filming the wrong window.
"""
import os
import subprocess
import sys
import time
import traceback

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import desk  # noqa: E402
import rp  # noqa: E402
from pointer import Pointer  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))

SEED = os.environ.get('SHOWREEL_MCP_SEED', 'Bring Me The Horizon')
PREFIX = f'Like {SEED}'
# Ten, not a hundred. The shot is the row arriving; the list behind it is read
# in the next shot, not counted, and ten is written back in a blink.
TARGET = os.environ.get('SHOWREEL_MCP_TARGET', '10')
# The server that matches the running app, not this branch's build: the live
# database is ahead of the branch and the branch binary refuses to open it.
MCP = os.environ.get('REPRISE_MCP', os.path.expanduser('~/.local/bin/reprise-mcp'))
DB = os.environ.get('REPRISE_DB',
                    os.path.expanduser('~/.local/share/reprise/reprise.db'))

UNTOUCHED = 4.0    # the library before anything happens; the cut lays the ask over it
APPEAR = 20.0      # how long the row is given to turn up
AFTER_ROW = 3.0    # the row is allowed to be seen before the hand goes for it
EASE = 1.1
SETTLE = 0.5
DWELL = 8.0


def start_cast(path, flag, budget=120):
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


def row_named(prefix):
    """The sidebar row whose name starts with `prefix`, and its rect.

    By name, never by a held node: the sidebar's accessible objects are rebuilt
    while the app runs. By prefix, because the playlist carries the track count
    in its name and the count is what the server decided, not what was asked.
    """
    for name, rect in desk.row_map().items():
        if name.startswith(prefix):
            return name, rect
    return None, None


def wait_for_row(prefix, timeout=APPEAR, poll=0.5):
    deadline = time.time() + timeout
    while True:
        name, rect = row_named(prefix)
        if name is not None:
            return name, rect
        if time.time() >= deadline:
            return None, None
        time.sleep(poll)


def main():
    dry = '--dry' in sys.argv

    for path in (MCP, DB):
        if not os.path.exists(path):
            sys.exit(f'ABORT: missing {path}')
    existing, _ = row_named(PREFIX)
    if existing:
        sys.exit(f'ABORT: {existing!r} is already in the sidebar — the shot is '
                 f'a row appearing, so it must not be there beforehand')
    if dry:
        print(f'ok: {MCP} and {DB} exist, no {PREFIX!r} row yet', flush=True)
        return 0

    if not desk.bring_to_front():
        sys.exit('ABORT: could not bring Reprise to the front')
    origin, (fw, fh) = desk.window_origin(desk.active_frame())
    print(f'window {fw}x{fh}, origin {origin}', flush=True)
    desk.wait_quiescent()

    work = rp.work_dir()
    asked = os.path.join(work, 'roh-gnome-mcp-hand.mp4')
    flag = os.path.join(work, 'stop-mcp-hand.flag')
    for stale in (asked, asked + '.mp4', flag):
        if os.path.exists(stale):
            os.remove(stale)
    timeline = os.path.join(work, 'timeline-mcp-hand.tsv')

    cast, film = start_cast(asked, flag)
    t0 = time.time()
    time.sleep(2.5)

    marks, failures = [], []

    def mark(label, note=''):
        marks.append((round(time.time() - t0, 2), label, note))
        print(f'[{label}] {time.time() - t0:.2f}s {note}', flush=True)

    p = Pointer()
    # Parked, in view and still. The hand is in the film from the first frame;
    # what it must not do is move while the claim is that nobody touched
    # anything.
    x, y = origin[0] + fw * 0.72, origin[1] + fh * 0.40
    try:
        p.move_to(x, y)
        mark('parked', f'({x:.0f},{y:.0f})')
        time.sleep(UNTOUCHED)

        mark('ask', f'{PREFIX!r} over the MCP server')
        built = subprocess.run(
            [sys.executable, os.path.join(HERE, 'mcp-playlist.py'),
             MCP, DB, PREFIX, SEED],
            capture_output=True, text=True,
            env=dict(os.environ, SHOWREEL_MCP_TARGET=TARGET))
        if built.returncode != 0:
            failures.append('mcp-playlist: '
                            + (built.stderr.strip().splitlines() or ['no output'])[-1])
        else:
            mark('written', (built.stderr.strip().splitlines() or [''])[-1])

        name, rect = wait_for_row(PREFIX)
        if name is None:
            failures.append(f'no {PREFIX!r} row appeared within {APPEAR:.0f}s')
        else:
            mark('appeared', repr(name))
            time.sleep(AFTER_ROW)
            for attempt in (1, 2):
                if attempt > 1:
                    # The pointer is positioned by dead reckoning — there is no
                    # way to read it back under Wayland — so one touch of the
                    # real mouse during a take silently offsets every later
                    # aim by that much. The first attempt of this shot missed
                    # by (126, 484) for exactly that reason. Re-homing throws
                    # the assumption away and takes the corner clamp as the
                    # truth again; it costs a visible flight to the corner,
                    # which is cheaper than a take that clicks at nothing.
                    p.home()
                    x, y = float(p.x), float(p.y)
                aim = desk.settled_row_centre(name, origin)
                if aim is None:
                    failures.append(f'{name!r} left the sidebar again')
                    break
                tx, ty = aim
                mark('reach', f'-> ({tx:.0f},{ty:.0f})')
                p.ease_to(x, y, tx, ty, seconds=EASE)
                x, y = tx, ty
                time.sleep(SETTLE)
                if desk.wait_active() is None:
                    failures.append('lost focus before the click')
                    break
                p.click()
                time.sleep(1.4)
                selected = desk.selected_row()
                if selected and selected.startswith(PREFIX):
                    mark('opened', repr(selected))
                    break
                if attempt == 2:
                    failures.append(f'the row did not open (selected {selected!r})')
            time.sleep(DWELL)
        mark('end')
    except Exception:
        failures.append('aborted: '
                        + traceback.format_exc().strip().splitlines()[-1])
        traceback.print_exc()
    finally:
        p.close()
        time.sleep(1.5)
        open(flag, 'w').close()
        cast.wait(timeout=60)
        with open(timeline, 'w') as fh:
            for t, label, note in marks:
                fh.write(f'{label}\t{t}\t{note}\n')
        print(f'TIMELINE {timeline}', flush=True)
        print(f'FILM {film}', flush=True)
        for f in failures:
            print(f'FAIL {f}', flush=True)
        print(f'VERDICT {"PASS" if not failures else "FAIL"}', flush=True)
    return 0 if not failures else 1


if __name__ == '__main__':
    sys.exit(main())
