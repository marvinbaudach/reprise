#!/usr/bin/env python3
"""Measure a generated track against the cut's own grid.

A model asked for 100 BPM does not return exactly 100 BPM, and the film's every
edit sits on a 0.6 s beat. Close is not good enough: a one percent error walks
a third of a beat across half a minute, which is exactly long enough to hear as
sloppiness without being obviously wrong.

So the track is measured rather than trusted. This prints the tempo it actually
has and the offset of its first downbeat, plus the `atempo` factor and trim that
put its beats on the cut's.

Usage: align-bed.py TRACK [TARGET_BPM] [TARGET_SECONDS]
"""
import subprocess
import sys

import numpy as np

SR = 22_050            # plenty for an onset envelope, and a quarter of the work
HOP = 256              # ~86 frames a second
FPS = SR / HOP


def load_mono(path):
    raw = subprocess.run(
        ['ffmpeg', '-v', 'error', '-i', path, '-ac', '1', '-ar', str(SR), '-f', 'f32le', '-'],
        check=True, stdout=subprocess.PIPE).stdout
    return np.frombuffer(raw, dtype='<f4').astype(np.float64)


def onset_envelope(x):
    """Spectral flux: how much energy appeared since the last frame. Only the
    rises count — a note ending is not an onset."""
    window = np.hanning(1024)
    frames = 1 + (len(x) - 1024) // HOP
    spec = np.empty((frames, 513))
    for i in range(frames):
        spec[i] = np.abs(np.fft.rfft(x[i * HOP : i * HOP + 1024] * window))
    spec = np.log1p(spec * 10.0)
    flux = np.maximum(np.diff(spec, axis=0), 0.0).sum(axis=1)
    flux -= flux.mean()
    return flux / (flux.std() + 1e-9)


def estimate_tempo(flux, low=70.0, high=170.0):
    """Autocorrelation of the onset envelope, read only over plausible lags.

    The peak is interpolated rather than taken at its integer lag. At 100 BPM a
    beat is 51.7 frames long, so rounding to 52 alone reports 99.4 BPM — a 0.6%
    error, which is most of the drift this script exists to remove. Measured
    against a synthesised 100.00 BPM control: 99.38 before, 99.85 after — and
    the onset energy landing on the estimated beats rose from 0.61 to 2.47,
    which is the sharper signal that the tempo is now right."""
    corr = np.correlate(flux, flux, mode='full')[len(flux) - 1 :]
    lags = np.arange(len(corr))
    with np.errstate(divide='ignore'):
        bpm = 60.0 * FPS / lags
    usable = (bpm >= low) & (bpm <= high)
    index = np.flatnonzero(usable)[np.argmax(corr[usable])]

    # Parabola through the peak and its two neighbours; its vertex is the lag.
    y0, y1, y2 = corr[index - 1], corr[index], corr[index + 1]
    denominator = y0 - 2 * y1 + y2
    shift = 0.5 * (y0 - y2) / denominator if denominator != 0 else 0.0
    lag = index + np.clip(shift, -0.5, 0.5)
    return 60.0 * FPS / lag, corr[usable].max() / (corr[0] + 1e-9)


def best_phase(flux, bpm):
    """Slide a pulse train at the measured tempo and keep the offset whose
    beats sit on the most onset energy."""
    period = 60.0 / bpm * FPS
    offsets = np.arange(0, int(round(period)))
    scores = []
    for offset in offsets:
        beats = np.round(np.arange(offset, len(flux), period)).astype(int)
        beats = beats[beats < len(flux)]
        scores.append(flux[beats].mean() if len(beats) else -np.inf)
    return offsets[int(np.argmax(scores))] / FPS, float(np.max(scores))


def main():
    path = sys.argv[1]
    target_bpm = float(sys.argv[2]) if len(sys.argv) > 2 else 100.0
    target_seconds = float(sys.argv[3]) if len(sys.argv) > 3 else 31.2

    audio = load_mono(path)
    flux = onset_envelope(audio)
    bpm, strength = estimate_tempo(flux)
    offset, punch = best_phase(flux, bpm)

    # atempo speeds up when the factor is above one, so the target goes in the
    # numerator: a track at 101 BPM has to be slowed to reach 100, which is a
    # factor below one. With the ratio the other way round every track near 100
    # BPM was corrected in the wrong direction by a fraction of a percent -- too
    # small to hear, which is why it stood -- and a 133 BPM take was sped up to
    # 177 and ran out of material at 45 s, after which score.sh padded the film
    # with fifteen seconds of silence.
    factor = target_bpm / bpm
    # After the stretch the downbeat moves with everything else.
    trim = offset / factor

    print(f'file        {path}')
    print(f'duration    {len(audio) / SR:.3f} s')
    print(f'tempo       {bpm:.2f} BPM   (confidence {strength:.3f})')
    print(f'downbeat    {offset:.3f} s  (onset energy on the beat {punch:.2f})')
    print(f'atempo      {factor:.6f}')
    print(f'trim start  {trim:.3f} s')
    print(f'usable      {len(audio) / SR / factor - trim:.3f} s of {target_seconds} needed')


if __name__ == '__main__':
    main()
