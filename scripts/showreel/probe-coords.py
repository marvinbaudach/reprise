#!/usr/bin/env python3
"""Find out what coordinate space ydotool's absolute move actually lives in.

AT-SPI hands out window-relative geometry that is correct and screen geometry
that is all zeroes — a Wayland client does not know where it sits. So a click
has to be aimed with window coordinates plus the window's own origin, and that
only works if ydotool's `-a` takes the same logical pixels the compositor uses
rather than a normalised range.

This does not guess. It parks the pointer at three known points with a pause on
each, films it with the cursor drawn, and the frames say where the pointer went.
"""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import rp  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SOCKET = os.environ.get('YDOTOOL_SOCKET', f'/run/user/{os.getuid()}/.ydotool_socket')

# Logical screen is 1728x1080 (2880x1800 at scale 1.6667), the top bar is 32 px
# and the Reprise frame is 1728x1048 below it. If `-a` speaks logical pixels the
# pointer lands on these; if it speaks a normalised range they all collapse into
# the top-left corner and the frames will show that instead.
MARKS = [
    ('bottom-right', 1600, 1000),
    ('centre', 864, 540),
    ('podcasts-row', 119, 181),   # 6+227/2, 32+131+18
]


def ydotool(*args):
    return subprocess.run(['ydotool', *args],
                          env=dict(os.environ, YDOTOOL_SOCKET=SOCKET),
                          capture_output=True, text=True)


def main():
    work = rp.work_dir()
    cast = os.path.join(work, 'probe-coords.mp4')
    flag = os.path.join(work, 'probe-coords.flag')
    if os.path.exists(flag):
        os.remove(flag)

    rec = subprocess.Popen(
        [sys.executable, os.path.join(HERE, 'screencast.py'), cast, flag, '30'],
        env=dict(os.environ, SHOWREEL_DRAW_CURSOR='1'),
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    time.sleep(3.0)

    stamps = []
    t0 = time.time()
    for name, x, y in MARKS:
        ydotool('mousemove', '-a', '-x', str(x), '-y', str(y))
        stamps.append((name, x, y, time.time() - t0))
        print(f'{name}: asked for ({x},{y}) at t+{stamps[-1][3]:.1f}s', flush=True)
        time.sleep(2.5)

    open(flag, 'w').close()
    try:
        rec.wait(timeout=20)
    except subprocess.TimeoutExpired:
        rec.kill()
    print(f'recording -> {cast}', flush=True)
    # The screencast starts about 3 s before the first mark; the sampler adds
    # that offset itself rather than baking it in here.
    for name, x, y, t in stamps:
        print(f'SAMPLE {name} {x} {y} {t + 3.0:.2f}', flush=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
