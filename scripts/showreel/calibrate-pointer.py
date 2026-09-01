#!/usr/bin/env python3
"""Where does the pointer actually end up? Measured off the film, not argued.

`pointer.py` has no closed loop — under Wayland the X root pointer is stale over
native windows, so nothing can read the cursor back. The position is dead
reckoning from a clamped corner, and dead reckoning is only as good as the
assumption that a commanded delta arrives 1:1.

That assumption was checked once, with a single large jump, and held. The first
film driven by an *eased* move showed it does not hold there: the clicks landed
on rows the run never aimed at. This measures both paths against the same ruler.

The ruler is the recording itself, because it is the only thing that sees the
real cursor: `org.gnome.Shell.Screenshot` is refused on this desktop
("Screenshot is not allowed"), and the ScreenCast/PipeWire path is the trap the
plan already documents. So the run parks the cursor at a series of stations,
films them, and diffs each station against a reference frame with the cursor
homed in the corner. With the UI held still, the only thing that differs is the
cursor.

Must be started detached: a foreground Bash call raises the terminal.
"""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import numpy as np  # noqa: E402
from scipy import ndimage  # noqa: E402
from PIL import Image  # noqa: E402

import desk  # noqa: E402
import rp  # noqa: E402
from pointer import Pointer  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
HOLD = 2.0          # a station is held long enough that a ±0.5 s sampling error still lands inside
SETTLE = 0.35
TOPBAR = 70         # physical rows: the clock changes minutes and would diff
CORNER = 260        # physical box at bottom-right: the homed cursor lives there in the reference
CUR_MIN, CUR_MAX = 12, 64   # a cursor's bbox in physical pixels; a hover bar is far wider


def start_cast(path, flag, budget=180):
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


def frame(film, t, out):
    subprocess.run(['ffmpeg', '-v', 'error', '-ss', f'{t:.2f}', '-i', film,
                    '-frames:v', '1', '-y', out], check=True)
    return np.asarray(Image.open(out).convert('L'), dtype=np.int16)


def cursor_at(ref, cur):
    """Find the cursor by its shape, not by what changed.

    The first version took the bounding box of every changed pixel and was
    wrong on nine stations out of ten: moving the pointer over the app lights a
    hover highlight under it, so the diff is dominated by a 660x60 bar and the
    reading is the bar's corner. Diffing only says *something* moved.

    A cursor is small, compact and contains near-white pixels; a hover
    highlight is wide, flat and dim. So the diff is only a candidate mask — the
    components are then filtered by that shape, and the brightest survivor
    wins. Returns the bbox top-left in physical pixels, which for the standard
    arrow is the hotspot.
    """
    d = np.abs(cur - ref)
    d[:TOPBAR, :] = 0
    d[-CORNER:, -CORNER:] = 0
    mask = d > 60
    if not mask.any():
        return None, 'nothing changed'
    labels, n = ndimage.label(mask)
    if n == 0:
        return None, 'no components'
    best, best_score, seen = None, -1.0, []
    for sl, idx in zip(ndimage.find_objects(labels), range(1, n + 1)):
        if sl is None:
            continue
        h = sl[0].stop - sl[0].start
        w = sl[1].stop - sl[1].start
        area = int((labels[sl] == idx).sum())
        seen.append((w, h, area))
        if not (CUR_MIN <= w <= CUR_MAX and CUR_MIN <= h <= CUR_MAX):
            continue
        if area < 120:
            continue
        bright = float(cur[sl][labels[sl] == idx].max())
        if bright < 200:            # the arrow is white; a highlight is not
            continue
        if bright > best_score:
            best, best_score = (int(sl[1].start), int(sl[0].start)), bright
    if best is None:
        big = sorted(seen, key=lambda t: -t[2])[:3]
        return None, f'no cursor-shaped component; largest {big}'
    return best, f'{n} components, brightest {best_score:.0f}'


def main():
    work = rp.work_dir()
    asked = os.path.join(work, 'calib.mp4')
    flag = os.path.join(work, 'stop-calib.flag')
    for stale in (asked, asked + '.mp4', flag):
        if os.path.exists(stale):
            os.remove(stale)

    if not desk.bring_to_front():
        sys.exit('ABORT: could not bring Reprise to the front')
    origin, (fw, fh) = desk.window_origin(desk.active_frame())
    sw, sh = desk.logical_screen()
    print(f'window {fw}x{fh}, origin {origin}, screen {sw}x{sh}', flush=True)

    # Stations spread over the window, all on dark chrome rather than text.
    jump_targets = [(300, 300), (900, 250), (1400, 700), (200, 800), (700, 500)]
    ease_start = (origin[0] + fw * 0.70, origin[1] + fh * 0.35)
    ease_targets = [(120, 181), (120, 257), (120, 561), (900, 250)]

    cast, film = start_cast(asked, flag)
    # The film's zero is the moment the recording started, not the moment the
    # driving starts. Timing station frames from the latter sampled every
    # station one place too early, and every reading came back as the previous
    # station's target — which read exactly like a drifting pointer and was
    # nothing of the kind.
    t0 = time.time()
    time.sleep(2.5)
    stations = []

    def station(kind, commanded):
        time.sleep(SETTLE)
        stations.append((kind, commanded, time.time() - t0 + HOLD / 2))
        time.sleep(HOLD)

    p = Pointer()
    try:
        station('reference-home', (p.width, p.height))
        for tx, ty in jump_targets:
            p.home()
            p.move_to(tx, ty)
            station('jump', (tx, ty))
        for tx, ty in ease_targets:
            p.home()
            p.move_to(*ease_start)
            time.sleep(0.3)
            p.ease_to(ease_start[0], ease_start[1], tx, ty)
            station('ease', (tx, ty))
    finally:
        p.close()
        time.sleep(0.8)
        open(flag, 'w').close()
        cast.wait(timeout=40)

    scale = None
    ref = None
    print(f'FILM {film}', flush=True)
    tmp = os.path.join(work, 'calib-frame.png')
    for kind, commanded, t in stations:
        img = frame(film, t, tmp)
        if scale is None:
            scale = img.shape[1] / sw
            print(f'film {img.shape[1]}x{img.shape[0]}, scale {scale:.4f}', flush=True)
        if kind == 'reference-home':
            ref = img
            continue
        hit, why = cursor_at(ref, img)
        if hit is None:
            print(f'{kind:<5} asked {commanded}  -> UNREADABLE: {why}', flush=True)
            continue
        ax, ay = hit[0] / scale, hit[1] / scale
        cx, cy = commanded
        print(f'{kind:<5} asked ({cx:>4},{cy:>4})  actual ({ax:7.1f},{ay:7.1f})  '
              f'drift ({ax - cx:+7.1f},{ay - cy:+7.1f})  [{why}]', flush=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
