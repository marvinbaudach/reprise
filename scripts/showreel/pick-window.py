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


def arc_steps(seconds):
    """The film's own dynamic shape, as (second, level) breakpoints.

    Read off the shot list in cut-film.sh, and it has to be re-read whenever
    that list changes — these were the 31.2 s cut's, then the 60.0 s cut's, and
    a window scored against the wrong shape is a measurement of nothing.

    The 58.2 s edit: the intro card holds for three seconds, the hook lands at
    8.4, the desktop run carries to 24.6, the last page pulls back, the handover
    at 30.0 is the break, the phone section stands alone from 37.8, and the end
    card falls away from 54.6.

    Library Doctor left and the five desk shots that remain hold 5.4 s, so the
    desk run ends at 30.0 rather than 31.8. The 1.8 s went into the handover's
    desk half, which is why the slide is still at 36.6 and the bed did not have
    to be re-spliced: `spliced-58s.wav` has the same A segment as the 51 s one
    (36.5162 s), so the bass still lands at 36.5.

    Shorter than the 66.6 s edit because four shots left it, not because anything
    was tightened: the search shot showed the Music page a second time, two of
    the four phone shots were Android navigation the desktop half already makes
    the case for, and the agent shot asked more reading than four seconds buys.

    The break is longer than it was. The handover used to be 2.4 s and the slam
    came at 43.8; the device-sync page now holds 6.2 s of it, so the recovery
    runs from the break at 41.4 up to the slide at 47.4 and the film arrives on
    the phone at full level. `score.sh` reads the breakpoint after the dip as the
    drop's release, so that one number is also where the filter opens again —
    moving 43.8 to 47.4 is what keeps the drop on the cut to the phone instead
    of leaving it three seconds early, in the middle of the sync page.

    The phone section holds. It used to interpolate straight from the handover's
    slam down to the end card's 0.30, which is a fifteen-second fade across the
    whole phone half — every phone shot quieter than the one before it, for no
    reason in the picture. The breakpoint at 63.0 is what makes it a hold and
    then a fall, which is what the sentence above always said it was.

    `arc-gain.py` reads these too, so the shape a window is chosen for and the
    shape it is then given on the fader are the same one.
    """
    return [(0.0, 0.35), (3.0, 0.72), (8.4, 1.0), (24.6, 0.85),
            (30.0, 0.32), (36.6, 1.0), (54.6, 0.95), (seconds, 0.22)]


def target_arc(seconds, rate):
    """The shape above, sampled at the envelope's own rate."""
    steps = arc_steps(seconds)
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
