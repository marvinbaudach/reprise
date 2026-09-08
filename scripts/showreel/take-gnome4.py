#!/usr/bin/env python3
"""The desktop tour, driven by a real pointer that the film can see.

Replaces the AT-SPI `do_action` drivers. Those reached their element whatever
the window stacking was, which is exactly why the old film has no hand in it:
pages turned with nothing on screen causing them.

Shape of the take, per the plan:

  * nine desktop stations, then the device sync as the handover
  * the track is loaded and **paused** — the player bar is dressed but the
    playhead does not move, so the cuts stop announcing themselves (R3)
  * every station holds long enough that the cut can find a 4.8 s window which
    contains the pointer arriving, the click landing and the page turning (R4)

Run it detached. A foreground Bash call raises the terminal and the focus guard
aborts the take rather than filming the wrong window.

    python3 take-gnome4.py --dry    resolve the targets and print them
    python3 take-gnome4.py --limit 4    shoot the first four stations only
    python3 take-gnome4.py          shoot
"""
import os
import subprocess
import sys
import time
import traceback

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import gi  # noqa: E402
gi.require_version('Atspi', '2.0')
from gi.repository import Atspi  # noqa: E402

import desk  # noqa: E402
import rp  # noqa: E402
from pointer import Pointer  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))

# Nine stations, then the handover. Each is (label, sidebar row name).
# Queue is not one of them any more. It is the one page in the sidebar that
# shows the film's own furniture back to itself — a list of what is playing,
# next to a player bar already saying so — and it was the page the eye kept
# landing on in the right-hand panel too.
STATIONS = [
    ('library', 'Music'),
    ('podcasts', 'Podcasts'),
    ('youtube', 'YouTube'),
    ('radio', 'Radio'),
    ('releases', 'Releases'),
    ('concerts', 'Concerts'),
    ('stats', 'My Stats'),
    ('doctor', 'Library Doctor'),
]
# The sidebar is not a constant. 'Library Doctor' is not in it on the
# 2026-09-02 dev nightly, and a station the take cannot resolve aborts the run
# — correctly, but that also makes it impossible to shoot the handover, which
# only happens when no --limit is given. So the list is filterable by label:
#
#   SHOWREEL_STATIONS=library,podcasts   two stations, then the handover
#   SHOWREEL_STATIONS=                   the handover alone
#
# Unset means all of them, which is what every earlier take did.
_WANTED = os.environ.get('SHOWREEL_STATIONS')
if _WANTED is not None:
    _KEEP = [s for s in _WANTED.split(',') if s]
    STATIONS = [row for row in STATIONS if row[0] in _KEEP]
# The device is reached by a *button*, not a sidebar list item, and its
# accessible name carries the verb: asking for a 'list item' named
# 'Pixel 10 Pro XL' finds nothing, which is how the dry run caught this before
# it cost a take. The start button is 'Sync now'.
DEVICE_BUTTON = 'Open Pixel 10 Pro XL'
SYNC_BUTTON = 'Sync now'
# Clicking the button starts a real transfer to a real phone, and the MCP
# state's `can_start: false` is not a reliable promise that the button is
# absent: one take found it, clicked it and synced. So the click is opt-in.
# With it off the station still drives to the device page and holds the same
# length — the shot is the page, not the transfer.
SYNC_CLICK = os.environ.get('SHOWREEL_SYNC_CLICK', '') not in ('', '0', 'no')
# The film wants no Recreant beside every shot, but this click does not
# deliver it: measured on two takes, one starting with the panel open and one
# with it closed by hand, the pointer lands on the toggle's own centre and the
# panel does not move — and neither does the toggle's CHECKED state, which is
# never set either. Left in and off by default, because a click that did work
# would re-open a panel someone had just closed. Close it by hand until the
# toggle can be driven; the proof is a frame of the take, never the property.
PANEL_TOGGLE = 'Toggle info panel'
PANEL_OFF = os.environ.get('SHOWREEL_PANEL_OFF', '0') not in ('0', 'no')
# While a sync runs there is no start button — the page offers 'Cancel' and the
# device's own name instead. Taking the start button as the proof that the page
# opened therefore fails hardest exactly when the shot is at its best: the tour
# reached a device page with a live transfer on it and called that a missing
# widget. The page is proven by what is always on it.
DEVICE_PAGE = ('eject', 'cancel')

DWELL = 6.0        # a 4.8 s shot needs room on both sides of the change
# The sync is the handover and the only shot whose subject is *time passing*.
# It is filmed long and compressed in the cut, so the progress bar visibly
# travels instead of creeping.
SYNC_DWELL = float(os.environ.get('SHOWREEL_SYNC_DWELL', '45'))
EASE = 1.1
SETTLE = 0.5
RESOLVE_SWEEPS = 4   # a blink in the sidebar must not cost a take
RESOLVE_WAIT = 2.0


# 240 s was the budget until a tour that retried two stations ran 300 s and the
# recording stopped at 240 — with a PASS verdict, a full timeline, and no
# handover in the film at all. The budget is a ceiling on a run whose length
# depends on how many stations need a second attempt, so it is set well above
# the longest run seen rather than just above the usual one.
def start_cast(path, flag, budget=480):
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


def row(name):
    return rp.find(name, role='list item')


def button_matching(*words):
    """A button whose name contains any of the words, case-insensitively.

    The role is `button` here, not `push button` — asking for the latter finds
    nothing and looks exactly like a missing widget.
    """
    for n in rp.walk(rp.app_root()):
        try:
            if n.get_role_name() != 'button':
                continue
            nm = (n.get_name() or '').lower()
        except Exception:
            continue
        if any(w in nm for w in words):
            return n
    return None


def limit_from_argv():
    """`--limit N`: the first N sidebar stations, and no device handover.

    Two uses, one switch. A four-station run crosses the station that killed
    the first take in well under a minute, which is what a fix should be proven
    on before a full take is spent on it; and when the phone is not connected,
    the nine desktop stations are still shootable while the handover is not.
    """
    if '--limit' not in sys.argv:
        return None
    n = int(sys.argv[sys.argv.index('--limit') + 1])
    if not 1 <= n <= len(STATIONS):
        raise SystemExit(f'ABORT: --limit must be 1..{len(STATIONS)}')
    return n


def device_page_open():
    """Is the device page up? Answered by a widget that is always on it.

    Not by the start button: that one is absent for the whole duration of a
    running sync, which is the one state this shot most wants to film.
    """
    return button_matching(*DEVICE_PAGE) is not None


def resolve(limit=None):
    """Every target the take needs, checked before a single frame is shot.

    It returns *names*, never the nodes it found them by: a node resolved here
    is dead by the third station, because a page turn rebuilds the sidebar's
    accessible objects. This pass only answers "is every target there", and
    each station looks its own target up again at the moment it aims.
    """
    wanted = [(label, name, 'list item') for label, name in STATIONS[:limit]]
    if limit is None:
        wanted.append(('sync', DEVICE_BUTTON, 'button'))

    # A target that answers on the second sweep is not a missing target. The
    # sidebar rebuilds itself while the app runs, and one sweep caught the top
    # three rows gone and everything below them present; they were all back
    # seconds later. Aborting a take over that would be aborting over a blink.
    missing = wanted
    for attempt in range(RESOLVE_SWEEPS):
        if attempt:
            time.sleep(RESOLVE_WAIT)
        missing = [t for t in missing if rp.find(t[1], role=t[2]) is None]
        if not missing:
            break
    gone = {(label, name) for label, name, _ in missing}
    found = [(label, name) for label, name, _ in wanted
             if (label, name) not in gone]
    return found, sorted(gone, key=lambda t: t[0])


def station_centre(label, name, origin):
    """Where to aim, read fresh — the rect is only true at the moment of use."""
    if label == 'sync':
        node = rp.find(name, role='button')
        return desk.centre_of(node, origin) if node is not None else None
    return desk.settled_row_centre(name, origin)


def main():
    dry = '--dry' in sys.argv
    limit = limit_from_argv()

    if not desk.bring_to_front():
        sys.exit('ABORT: could not bring Reprise to the front')
    origin, (fw, fh) = desk.window_origin(desk.active_frame())
    print(f'window {fw}x{fh}, origin {origin}', flush=True)

    # Before the camera, never during it: waiting for the sidebar to stop
    # moving costs seconds, and a take that waits on film is a take with a
    # hole in it.
    desk.wait_quiescent()
    found, missing = resolve(limit)
    if limit is not None:
        print(f'limit {limit}: {limit} station(s), no device handover', flush=True)
    for label, name in found:
        print(f'  ok      {label:<9} {name!r}', flush=True)
    for label, name in missing:
        print(f'  MISSING {label:<9} {name!r}', flush=True)
    if dry:
        return 1 if missing else 0
    if missing:
        sys.exit(f'ABORT: {len(missing)} target(s) missing — fix the list, do not film around it')

    work = rp.work_dir()
    stem = 'roh-gnome-tour' if limit is None else f'roh-gnome-{limit}'
    asked = os.path.join(work, f'{stem}.mp4')
    flag = os.path.join(work, f'stop-{stem}.flag')
    for stale in (asked, asked + '.mp4', flag):
        if os.path.exists(stale):
            os.remove(stale)
    timeline = os.path.join(work, f'timeline-{stem}.tsv')

    cast, film = start_cast(asked, flag)
    t0 = time.time()
    time.sleep(2.5)

    marks, failures = [], []

    def mark(label, note=''):
        marks.append((round(time.time() - t0, 2), label, note))
        print(f'[{label}] {time.time() - t0:.2f}s {note}', flush=True)

    p = Pointer()
    x, y = origin[0] + fw * 0.72, origin[1] + fh * 0.40
    try:
        p.move_to(x, y)
        time.sleep(1.2)
        # The info panel stands open beside every shot unless it is closed
        # here. Neither `ui.info_panel_visible=0` before launch nor an AT-SPI
        # `do_action` moves it — the key is written back on exit and the action
        # returns True while nothing happens — so the toggle is clicked with
        # the same pointer that drives the tour, and the proof is a frame of
        # the take itself, never the toggle's `checked` state.
        if PANEL_OFF:
            tog = rp.find(PANEL_TOGGLE)
            if tog is None:
                failures.append(f'panel: no {PANEL_TOGGLE!r} to click')
            else:
                px, py = desk.centre_of(tog, origin)
                p.ease_to(x, y, px, py, seconds=EASE)
                x, y = px, py
                time.sleep(SETTLE)
                p.click()
                mark('panel-off', f'clicked ({px:.0f},{py:.0f})')
                time.sleep(2.0)
        for label, name in found:
            for attempt in (1, 2):
                if attempt > 1:
                    # Dead reckoning is the only way to position the pointer
                    # under Wayland, so a real mouse moved during the take
                    # offsets every aim after it. A retry that eases from the
                    # same wrong assumption misses the same way; re-homing
                    # makes the corner clamp the truth again.
                    p.home()
                    x, y = float(p.x), float(p.y)
                aim = station_centre(label, name, origin)
                if aim is None:
                    failures.append(f'{label}: {name!r} is no longer there')
                    break
                tx, ty = aim
                mark(label, f'-> ({tx:.0f},{ty:.0f})')
                p.ease_to(x, y, tx, ty, seconds=EASE)
                x, y = tx, ty
                time.sleep(SETTLE)
                if desk.wait_active() is None:
                    failures.append(f'{label}: lost focus')
                    break
                p.click()
                time.sleep(1.2)
                if label == 'sync':
                    # The device page has no selected sidebar row to check, so
                    # the proof it opened is a widget the page always carries.
                    if device_page_open():
                        break
                    if attempt == 2:
                        failures.append('sync: device page did not open')
                    continue
                if desk.selected_row() == name:
                    break
                if attempt == 2:
                    failures.append(f'{label}: {name!r} did not select '
                                    f'(got {desk.selected_row()!r})')
            if label == 'sync':
                btn = button_matching(SYNC_BUTTON.lower()) if SYNC_CLICK else None
                if btn is not None:
                    e = btn.get_extents(Atspi.CoordType.WINDOW)
                    bx = origin[0] + e.x + e.width / 2
                    by = origin[1] + e.y + e.height / 2
                    mark('sync-start', f'{btn.get_name()!r} at ({bx:.0f},{by:.0f})')
                    p.ease_to(x, y, bx, by, seconds=EASE)
                    x, y = bx, by
                    time.sleep(SETTLE)
                    p.click()
                    time.sleep(SYNC_DWELL)   # filmed long, compressed in the cut
                elif device_page_open():
                    # Nothing to click — either the click is off, or a transfer
                    # is already running, which is the shot and not a failure:
                    # the phone auto-syncs when it is plugged in, so the tour
                    # can arrive to find the work already under way. Either way
                    # the station holds the same length on the page.
                    why = ('click disabled' if not SYNC_CLICK
                           else 'already syncing, no start button')
                    mark('sync-hold', why)
                    time.sleep(SYNC_DWELL)
                else:
                    failures.append('sync: no start button and no device page')
            time.sleep(DWELL)
        mark('end')
    except Exception:
        # A take that dies mid-tour is still worth its frames, but only with
        # its index: the first one threw past the timeline write and left a
        # 31 s film that nothing could cut. The reason goes in the failures so
        # the verdict says it out loud.
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
