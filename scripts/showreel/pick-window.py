#!/usr/bin/env python3
"""Choose which stretch of a generated track goes under the film.

The generator ignores the requested length — a 31.2 s request came back as 78
and 88 second pieces — so the track is material, not a finished cue. What it
does keep is structure: quiet openings, builds, breakdowns. This finds the
window whose shape matches the film's, instead of taking the first 31 seconds
and hoping.

The film's arc is fixed by its own edit: a held statement, the feature run, the
handover to the phone, the end card. A window is scored by how well its loudness
envelope correlates with that arc, and only windows starting on a beat are
considered, so the cue lands on the cut's grid either way.

Usage: pick-window.py TRACK [SECONDS] [BPM]
"""
import subprocess
import sys

import numpy as np

SR = 22_050
FRAME = SR // 50           # 20 ms of loudness envelope


def load_mono(path):
    raw = subprocess.run(
        ['ffmpeg', '-v', 'error', '-i', path, '-ac', '1', '-ar', str(SR), '-f', 'f32le', '-'],
        check=True, stdout=subprocess.PIPE).stdout
    return np.frombuffer(raw, dtype='<f4').astype(np.float64)


def envelope(x):
    frames = len(x) // FRAME
    block = x[: frames * FRAME].reshape(frames, FRAME)
    return np.sqrt((block**2).mean(axis=1) + 1e-12)


# How far a two-second stretch may sit below the window's own median before it
# counts as a hole rather than a breakdown. Measured against this material: a
# real breakdown lands around -16 dB, the generated track's own ending at -53.
HOLE_DB = -18.0
BLOCK = 100                # two seconds of envelope frames


def quietest_block(window):
    """The quietest two seconds of a window, in dB below its median."""
    usable = window[: len(window) // BLOCK * BLOCK].reshape(-1, BLOCK).mean(axis=1)
    return 20 * np.log10((usable.min() + 1e-12) / (np.median(usable) + 1e-12))


def target_arc(seconds, rate):
    """The film's own dynamic shape, in the same units as the envelope.

    Read off the shot list in cut-film.sh, and it has to be re-read whenever
    that list changes — the numbers below were the 31.2 s cut's until the film
    was recut to 60.0 s, and a window scored against the wrong shape is a
    measurement of nothing.

    The 60.0 s edit: the intro card holds for three seconds, the hook lands,
    the desktop run carries to 34.2, the agent shot pulls back, the handover
    to the phone at 39.0 is the break, the phone section stands alone from
    41.4, and the end card falls away from 56.4."""
    steps = [(0.0, 0.35), (3.0, 0.72), (7.2, 1.0), (34.2, 0.75),
             (39.0, 0.32), (41.4, 1.0), (56.4, 0.30), (seconds, 0.22)]
    times = np.arange(int(seconds * rate)) / rate
    return np.interp(times, [t for t, _ in steps], [v for _, v in steps])


def main():
    path = sys.argv[1]
    seconds = float(sys.argv[2]) if len(sys.argv) > 2 else 31.2
    bpm = float(sys.argv[3]) if len(sys.argv) > 3 else 100.0

    audio = load_mono(path)
    rate = SR / FRAME
    env = envelope(audio)
    arc = target_arc(seconds, rate)
    width = len(arc)
    if width > len(env):
        raise SystemExit(f'{path} is shorter than the {seconds}s it has to cover')

    beat = 60.0 / bpm
    starts = np.arange(0, (len(env) - width) / rate, beat)

    best = (-2.0, 0.0, 0.0)
    fallback = (-2.0, 0.0, -99.0)
    for start in starts:
        i = int(round(start * rate))
        window = env[i : i + width]
        if window.std() < 1e-9:
            continue
        # Correlation, not difference: what matters is that the track rises and
        # falls where the film does, not that it happens to sit at the same
        # absolute level. The level is set later, by loudnorm.
        score = float(np.corrcoef(window, arc)[0, 1])
        quiet = quietest_block(window)
        if score > fallback[0]:
            fallback = (score, start, quiet)
        # Shape alone is not enough. A generated track carries its own ending,
        # and a window that reaches past it correlates beautifully with an arc
        # that falls at the end while being, in fact, silent. The first cut of
        # the 60 s film had two seconds of nothing at 52 s and near-nothing
        # over the end card, and it scored better than every window that had
        # music all the way through. So a window that goes quiet is not a
        # candidate, however well it matches.
        if quiet < HOLE_DB:
            continue
        if score > best[0]:
            best = (score, start, quiet)

    score, start, quiet = best
    if score < -1.0:
        score, start, quiet = fallback
        print(f'warning no window of {path} stays above {HOLE_DB:.0f} dB '
              f'below its own median — the quietest is {quiet:.0f} dB',
              file=sys.stderr)
    print(f'file    {path}')
    print(f'length  {len(audio) / SR:.3f} s')
    print(f'start   {start:.3f} s')
    print(f'match   {score:.3f}')
    print(f'quietest {quiet:.1f} dB below median')


if __name__ == '__main__':
    main()
