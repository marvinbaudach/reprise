#!/usr/bin/env python3
"""Where in a recording does the film's bed start?

The phone take is shot while the handset plays the finished bed, so the take
carries the bed in its own audio. That is the only thing that says which frame
of the recording belongs to which second of the film — a stopwatch does not,
because the recording starts when scrcpy connects and the music starts when a
finger lands on play.

Prints one number: the second in the take at which film time 0 sits. Cut the
phone shots at `offset + film_time` and the visualiser is on the beat that is
actually heard under it.

    align-take.py TAKE.mp4 BED.m4a [--max-lag SECONDS]

Correlation is on the loudness envelope, not the waveform. The phone's speaker,
its enclosure and the AAC round trip all reshape the waveform; what survives all
three is where the music gets louder and quieter.
"""
import argparse
import subprocess
import sys

import numpy as np

SR = 8000
FRAME = 256


def envelope(path, seconds=None):
    """Loudness over time, at SR/FRAME samples per second."""
    cmd = ['ffmpeg', '-v', 'error']
    if seconds:
        cmd += ['-t', str(seconds)]
    cmd += ['-i', path, '-vn', '-ac', '1', '-ar', str(SR), '-f', 's16le', '-']
    raw = subprocess.run(cmd, capture_output=True, check=True).stdout
    audio = np.frombuffer(raw, dtype='<i2').astype(np.float32) / 32768.0
    if audio.size < FRAME:
        raise SystemExit(f'{path} carries no audio')
    width = audio.size // FRAME
    blocks = audio[: width * FRAME].reshape(width, FRAME)
    env = np.sqrt((blocks ** 2).mean(axis=1))
    # The absolute level differs — device speaker against a rendered file — and
    # only the shape is comparable, so both sides are standardised.
    return (env - env.mean()) / (env.std() + 1e-12)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('take')
    ap.add_argument('bed')
    ap.add_argument('--max-lag', type=float, default=None,
                    help='only consider offsets up to this many seconds')
    args = ap.parse_args()

    rate = SR / FRAME
    take = envelope(args.take)
    bed = envelope(args.bed)

    if take.size <= bed.size:
        raise SystemExit(
            f'the take is {take.size / rate:.1f}s and the bed is {bed.size / rate:.1f}s — '
            'the take has to be longer than the film for an offset to exist')

    limit = take.size - bed.size
    if args.max_lag is not None:
        limit = min(limit, int(args.max_lag * rate))

    # Sliding dot product of two standardised signals is their correlation, and
    # np.correlate does it in one pass instead of a Python loop over 20k offsets.
    scores = np.correlate(take, bed, mode='valid')[: limit + 1] / bed.size
    best = int(np.argmax(scores))
    offset = best / rate

    # A real lock is a spike. If the runner-up somewhere else is nearly as good,
    # the take probably does not carry the bed at all and the number is noise.
    #
    # The guard is four seconds, not one. The bed is two bars of 120 BPM
    # repeating, so at one second the runner-up is the neighbouring bar of the
    # same music — a correct alignment was rejected with match 0.885 against a
    # runner-up of 0.780 that sat 1.4 s away from it. Four seconds clears two
    # bars either side, which is where a genuinely different part of the track
    # starts.
    mask = np.ones(scores.size, dtype=bool)
    guard = int(4.0 * rate)
    mask[max(0, best - guard): best + guard] = False
    runner_up = float(scores[mask].max()) if mask.any() else 0.0

    print(f'offset {offset:.3f} match {scores[best]:.3f} runner-up {runner_up:.3f}')
    if scores[best] < 0.3 or runner_up > scores[best] * 0.85:
        print('no clear lock — check that the take really carries the bed', file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
