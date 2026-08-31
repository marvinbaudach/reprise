#!/usr/bin/env python3
"""Find the frame on which each station's page actually turns.

`take-gnome4.py` writes its timeline mark *before* the pointer starts moving,
so a mark is not an in-point: the reach, the click and the repaint all follow
it, and by an amount that belongs to the take, not to a constant anyone can
carry over from an older script.

This measures it. For every mark in the timeline the window around it is
decoded to small grayscale frames and the mean absolute difference between
neighbours is taken. A page turn repaints most of the content area at once and
stands far above the pointer's own few pixels, so the profile's peak is the
turn.

    python3 find-page-turns.py TAKE.mp4 TIMELINE.tsv
    python3 find-page-turns.py TAKE.mp4 TIMELINE.tsv --post 8

Control arm: run it on a take whose in-points are already known good and check
that peak-minus-in-point is the same number at every station. A profile whose
peak is not clearly above its own background is reported as WEAK rather than
answered.
"""
import argparse
import subprocess
import sys

import numpy as np

W, H = 320, 180


def frames(path, start, dur, fps):
    cmd = [
        'ffmpeg', '-v', 'error', '-ss', f'{start:.3f}', '-t', f'{dur:.3f}',
        '-i', path, '-vf', f'fps={fps},scale={W}:{H}', '-pix_fmt', 'gray',
        '-f', 'rawvideo', '-',
    ]
    raw = subprocess.run(cmd, capture_output=True, check=True).stdout
    n = len(raw) // (W * H)
    if n == 0:
        return np.empty((0, H, W), dtype=np.float32)
    return np.frombuffer(raw[:n * W * H], dtype=np.uint8).reshape(n, H, W).astype(np.float32)


def profile(path, start, dur, fps):
    f = frames(path, start, dur, fps)
    if len(f) < 3:
        return None, None
    d = np.abs(np.diff(f, axis=0)).mean(axis=(1, 2))
    t = start + (np.arange(len(d)) + 1.0) / fps
    return t, d


def read_marks(tsv):
    out = []
    for line in open(tsv):
        parts = line.rstrip('\n').split('\t')
        if len(parts) < 2:
            continue
        try:
            out.append((parts[0], float(parts[1])))
        except ValueError:
            continue
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('take')
    ap.add_argument('timeline')
    ap.add_argument('--pre', type=float, default=0.5)
    ap.add_argument('--post', type=float, default=6.5)
    ap.add_argument('--fps', type=float, default=20.0)
    ap.add_argument('--skip', default='end', help='comma-separated labels to skip')
    a = ap.parse_args()

    skip = {s for s in a.skip.split(',') if s}
    marks = [m for m in read_marks(a.timeline) if m[0] not in skip]
    if not marks:
        sys.exit('no usable marks in timeline')

    print(f'{"label":<10} {"mark":>7} {"turn":>7} {"lag":>6} {"peak":>7} {"bg":>6} {"ratio":>6}')
    for label, mark in marks:
        start = max(0.0, mark - a.pre)
        t, d = profile(a.take, start, a.pre + a.post, a.fps)
        if t is None:
            print(f'{label:<10} {mark:7.2f}   no frames')
            continue
        i = int(np.argmax(d))
        peak = float(d[i])
        # background = everything at least 0.4 s away from the peak
        far = np.abs(t - t[i]) > 0.4
        bg = float(np.median(d[far])) if far.any() else 0.0
        ratio = peak / bg if bg > 1e-6 else float('inf')
        flag = '' if ratio >= 4.0 else '   WEAK'
        print(f'{label:<10} {mark:7.2f} {t[i]:7.2f} {t[i] - mark:6.2f} '
              f'{peak:7.2f} {bg:6.2f} {ratio:6.1f}{flag}')


if __name__ == '__main__':
    main()
