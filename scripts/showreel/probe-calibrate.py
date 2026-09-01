#!/usr/bin/env python3
"""Measure what ydotool's absolute coordinates mean, instead of assuming.

`ydotool mousemove -a` may speak logical pixels, framebuffer pixels, or a
normalised range — its own help only warns about pointer acceleration. Aiming a
click from AT-SPI window coordinates needs the answer exactly, so this parks the
pointer on two widely separated points, films each with the cursor drawn, and
leaves two frames to read the mapping off.

It presses Escape first: the earlier probe drove the pointer into the top-left
hot corner and left the Activities overview open, which is both the wrong
picture and a window that is not Reprise.
"""
import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import rp  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
SOCKET = os.environ.get('YDOTOOL_SOCKET', f'/run/user/{os.getuid()}/.ydotool_socket')
KEY_ESC = 1

POINTS = [('A', 300, 250), ('B', 1400, 900)]


def ydotool(*args):
    return subprocess.run(['ydotool', *args],
                          env=dict(os.environ, YDOTOOL_SOCKET=SOCKET),
                          capture_output=True, text=True)


def record(path, seconds):
    """Run one screencast and return the file GNOME actually wrote.

    GNOME appends its own extension, so the requested name is not the name on
    disk — the previous probe looked for a file that was never going to exist.
    """
    flag = path + '.flag'
    if os.path.exists(flag):
        os.remove(flag)
    p = subprocess.Popen(
        [sys.executable, os.path.join(HERE, 'screencast.py'), path, flag, str(seconds + 10)],
        env=dict(os.environ, SHOWREEL_DRAW_CURSOR='1'),
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    time.sleep(seconds)
    open(flag, 'w').close()
    try:
        out, _ = p.communicate(timeout=25)
    except subprocess.TimeoutExpired:
        p.kill()
        out = ''
    real = ''
    for line in (out or '').splitlines():
        if line.startswith('RECORDING '):
            real = line.split(' ', 1)[1].strip()
    return real


def main():
    work = rp.work_dir()
    ydotool('key', f'{KEY_ESC}:1', f'{KEY_ESC}:0')
    time.sleep(1.0)

    for name, x, y in POINTS:
        ydotool('mousemove', '-a', '-x', str(x), '-y', str(y))
        time.sleep(0.6)
        real = record(os.path.join(work, f'calib-{name}'), 3.0)
        print(f'{name}: asked ({x},{y}) -> {real}', flush=True)
    return 0


if __name__ == '__main__':
    sys.exit(main())
