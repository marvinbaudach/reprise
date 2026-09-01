#!/usr/bin/env python3
"""Where in a silent recording does track time 0 sit?

Some phone takes have no usable audio — scrcpy cannot capture this app's
mixer output, so the recording's audio track is digital silence (measured
mean_volume -91.0 dB). align-take.py correlates loudness envelopes; with no
loudness on either side it has nothing to lock onto. The only clock left is
the one drawn on screen: the now-playing view's elapsed-position readout,
which ticks over once per second.

This does not read the digits. OCR on a 20x20 px anti-aliased readout is a
fight not worth having, and it does not need winning: the position advances
by exactly one second at a time, so it is enough to find *when* the readout
changes, not *what* it changes to. The first change is second 1, the second
is second 2, and so on — counting ticks is timing them.

Prints one line: `offset SECONDS slope SLOPE ticks N`. `offset` is the second
in the take at which track time 0 sits (i.e. where the readout was still
showing 0:00). Cut the phone shot at `offset + track_time` and it is on the
digit that was actually on screen at that instant.

Assumptions the shot has to honor, because nothing here can check them from
the pixels alone:

  - the recording starts on a paused 0:00 and playback begins inside the
    take, so the detected ticks are track seconds 1, 2, 3, ... in that exact
    order. Losing the first tick (a slow finger, a dropped frame right at
    the button press) does not break the linear fit — every later tick is
    still one second from its neighbours — it just shifts the whole result
    by -1.0 s with no residual to flag it. Watch the take's first second by
    eye before trusting the offset.
  - the readout truncates rather than rounds (typical for Android transport
    controls: 0:00 holds for the whole first second, not half of it either
    side of it). If the player rounds instead, every real offset is 0.5 s
    later than what this prints.

The crop box below is pixel coordinates on a Pixel 10 Pro XL in portrait,
1080x2404 at ~60 fps, for this one player's now-playing layout — elapsed
time bottom-left under the seek bar, remaining time bottom-right. A different
device, orientation or layout needs a new box, found the same way this one
was: crop a still generously around the readout, then tighten with a colour
threshold until only the digits survive.

    align-take-by-clock.py TAKE.mp4 [--fps 60] [--crop X,Y,W,H]
"""
import argparse
import subprocess
import sys

import numpy as np

# Pinned to p8.png: the "0:20" glyphs sit at x 61-118, y 1834-1854. The box
# below adds margin on every side — up to a 5-digit "12:34" readout on the
# right, and on top a 40 px gap down to the seek bar's teal track (y
# 1763-1770), so the moving playhead never enters the crop and manufactures a
# tick of its own.
DEFAULT_CROP = (40, 1810, 220, 90)

# roh-android-bed.mp4 shares p8.png's now-playing layout but not its geometry:
# the seek-bar playhead is a vertical tick mark whose bottom edge sits at
# y=1820 here, ten pixels *inside* DEFAULT_CROP's y=1810 top edge. For roughly
# the first 12 s of playback the playhead's x position also falls inside the
# crop's x range, so DEFAULT_CROP was catching the tail of that teal mark on
# every frame while it moved — not the visualiser, which lives up in the
# album-art square (y 594-1253) and never reaches this far down. That looked
# exactly like the failure mode the module warns about: motion inside the box
# that isn't the digits, timed faster than one tick a second. Moving the top
# edge down past the playhead (y=1825) and shrinking the height to just the
# glyphs (1834-1855, plus margin) leaves only the readout.
ROH_ANDROID_BED_CROP = (40, 1825, 220, 40)

# scrcpy's mp4 is variable frame rate; asking ffmpeg for a fixed output rate
# up front is what lets a frame index be divided by that same rate and get an
# exact take-time back. Without it, frame_index / nominal_fps drifts against
# wall-clock time by however much the source dropped or duplicated frames.
DEFAULT_FPS = 60.0

MIN_TICKS = 10
MAX_SLOPE_ERROR = 0.005     # 0.5%
MAX_RESIDUAL = 0.2          # seconds


def read_frames(path, fps, crop):
    """The cropped readout, one grayscale frame per output tick, as (n, h, w) uint8."""
    x, y, w, h = crop
    cmd = [
        'ffmpeg', '-v', 'error', '-i', path,
        '-vf', f'fps={fps},crop={w}:{h}:{x}:{y},format=gray',
        '-f', 'rawvideo', '-',
    ]
    raw = subprocess.run(cmd, capture_output=True, check=True).stdout
    if len(raw) < w * h:
        raise SystemExit(f'{path} produced no frames in the crop box')
    frames = np.frombuffer(raw, dtype=np.uint8)
    count = frames.size // (w * h)
    return frames[: count * w * h].reshape(count, h, w)


def find_ticks(frames, fps):
    """Frame indices where the readout changed, one per second, first-of-run only."""
    diffs = np.abs(frames[1:].astype(np.int16) - frames[:-1].astype(np.int16)).mean(axis=(1, 2))
    if diffs.size == 0:
        return np.array([], dtype=int)

    # The readout changes roughly once per second, so out of len(diffs) frame
    # gaps roughly one in `fps` is a real tick. Sorting the diffs and taking
    # the median of that top slice gives the typical size of a real change;
    # the median of everything gives the typical size of a non-change (video
    # noise, re-encode artifacts). The threshold sits a quarter of the way
    # from the quiet level up to the loud one — comfortably above encoder
    # noise, comfortably below a genuine digit swap, and it degenerates to 0
    # (nothing clears it) on a static readout instead of firing on every
    # frame, which a bare "multiple of the median" would do whenever the
    # median itself is 0.
    fps_guess = max(1, int(len(diffs) // fps))
    loud = np.median(np.sort(diffs)[::-1][:fps_guess])
    quiet = np.median(diffs)
    threshold = quiet + 0.25 * (loud - quiet)

    above = diffs > threshold
    ticks = []
    i = 0
    while i < above.size:
        if above[i]:
            # diffs[i] is the gap between frame i and frame i+1, so the run's
            # first raised gap means frame i+1 is the first frame carrying
            # the new digit. A cross-fade keeps `above` true for a few more
            # frames; skip the whole run so it collapses to one tick.
            ticks.append(i + 1)
            while i < above.size and above[i]:
                i += 1
        else:
            i += 1
    return np.array(ticks, dtype=int)


def fit_offset(times):
    """Least-squares take_time = offset + slope * k over k = 1, 2, 3, ..."""
    k = np.arange(1, times.size + 1, dtype=np.float64)
    slope, offset = np.polyfit(k, times, 1)
    residuals = times - (offset + slope * k)
    return offset, slope, residuals


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('take')
    ap.add_argument('--fps', type=float, default=DEFAULT_FPS)
    ap.add_argument('--crop', type=str, default=None,
                    help='X,Y,W,H overriding the pinned Pixel 10 Pro XL box')
    args = ap.parse_args()

    crop = DEFAULT_CROP
    if args.crop:
        crop = tuple(int(v) for v in args.crop.split(','))

    frames = read_frames(args.take, args.fps, crop)
    ticks = find_ticks(frames, args.fps)

    if ticks.size < MIN_TICKS:
        print(f'only {ticks.size} ticks found (need at least {MIN_TICKS}) — '
              'readout never changed, or the crop box missed it', file=sys.stderr)
        return 1

    times = ticks / args.fps
    offset, slope, residuals = fit_offset(times)

    if abs(slope - 1.0) > MAX_SLOPE_ERROR:
        print(f'slope {slope:.4f} is off from 1.0 by more than {MAX_SLOPE_ERROR * 100:.1f}% — '
              'ticks were probably miscounted', file=sys.stderr)
        return 1

    worst = float(np.max(np.abs(residuals)))
    if worst > MAX_RESIDUAL:
        print(f'a tick sits {worst:.3f}s off the fit (limit {MAX_RESIDUAL}s) — '
              'a change was likely missed or a spurious one detected, which mislabels '
              'every later tick by a whole second', file=sys.stderr)
        return 1

    print(f'offset {offset:.3f} slope {slope:.4f} ticks {ticks.size}')
    return 0


if __name__ == '__main__':
    sys.exit(main())
