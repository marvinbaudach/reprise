#!/usr/bin/env python3
"""Do the phone's bars move with the sound the viewer hears?

The claim of shot 13 is that the visualiser on the handset is following the
film's own music. Nothing in the pipeline enforces it: the take was recorded
while the phone played the bed, the cut chooses an in-point, and whether those
two land on the same beat is an accident until it is measured.

The on-screen clock is not the measurement. Aligning by it once put the
visualiser 0.6 s ahead of the sound and the readout gave no hint of it — the
readout ticks once a second and a second is two beats here.

So this measures the picture against the sound, on the finished cut, in the
two bands the visualiser draws in different colours:

  * the low third of the square against 40-160 Hz, which is the kick,
  * the high third against 1-3.5 kHz, which is where the cymbals live.

Both sides become one number per frame — the picture's is how much of the
band's column is lit, the sound's is the band's power — and the lag that makes
them agree best is the answer. A POSITIVE lag means the bars move BEFORE the
sound: the picture is ahead.

    measure-vis-sync.py CUT.mp4 START DUR [--crop W:H:X:Y] [--max-lag SECONDS]

The in-point that fixes a lead is `in_point - lead`: a later in-point shows a
further-advanced moment of the handset's playback at every film second, so it
adds to the lead rather than cancelling it (slope +1.0, measured over two
renders on 2026-08-29).
"""
import argparse
import subprocess
import sys

import numpy as np

FPS = 30
AUDIO_RATE = 16000
# The two bands the visualiser separates by colour. 40 Hz rather than 20: below
# it the bed has nothing but room, and the FFT bin at a 1/30 s hop is 30 Hz
# wide, so a lower edge would be one bin of noise.
LOW_BAND = (40.0, 160.0)
HIGH_BAND = (1000.0, 3500.0)


def video_bands(path, start, dur, crop):
    """One lit-pixel count per frame, for the low and the high third."""
    w, h, x, y = crop
    cmd = [
        'ffmpeg', '-v', 'error', '-ss', str(start), '-t', str(dur), '-i', path,
        '-vf', f'fps={FPS},crop={w}:{h}:{x}:{y}',
        '-f', 'rawvideo', '-pix_fmt', 'gray', '-',
    ]
    raw = subprocess.run(cmd, stdout=subprocess.PIPE, check=True).stdout
    frames = len(raw) // (w * h)
    if frames == 0:
        sys.exit('measure-vis-sync: no frames in that window')
    a = np.frombuffer(raw[: frames * w * h], dtype=np.uint8)
    a = a.reshape(frames, h, w).astype(np.float32)
    # The square's ground is dark and the bars are bright, so "lit" is a
    # threshold on luminance rather than an edge detector. The threshold is
    # taken from the shot itself — a fixed number would be a guess about
    # exposure, and the take is a screen recording of a screen.
    floor = np.percentile(a, 40)
    ceiling = np.percentile(a, 99)
    lit = np.clip((a - floor) / max(ceiling - floor, 1.0), 0.0, 1.0)
    third = w // 3
    return lit[:, :, :third].sum(axis=(1, 2)), lit[:, :, -third:].sum(axis=(1, 2))


def audio_bands(path, start, dur, frames):
    """One band power per frame, for the same two bands."""
    cmd = [
        'ffmpeg', '-v', 'error', '-ss', str(start), '-t', str(dur), '-i', path,
        '-map', '0:a:0', '-ac', '1', '-ar', str(AUDIO_RATE),
        '-f', 's16le', '-',
    ]
    raw = subprocess.run(cmd, stdout=subprocess.PIPE, check=True).stdout
    pcm = np.frombuffer(raw, dtype='<i2').astype(np.float32) / 32768.0
    if pcm.size == 0:
        sys.exit('measure-vis-sync: the cut has no audio — score it first')
    hop = AUDIO_RATE // FPS
    win = hop * 4          # 133 ms, four bins wide at the low edge
    window = np.hanning(win)
    freqs = np.fft.rfftfreq(win, 1.0 / AUDIO_RATE)
    lo = (freqs >= LOW_BAND[0]) & (freqs <= LOW_BAND[1])
    hi = (freqs >= HIGH_BAND[0]) & (freqs <= HIGH_BAND[1])
    low, high = [], []
    for i in range(frames):
        centre = i * hop
        a, b = centre - win // 2, centre + win // 2
        chunk = np.zeros(win, dtype=np.float32)
        src = pcm[max(a, 0):max(min(b, pcm.size), 0)]
        chunk[max(-a, 0):max(-a, 0) + src.size] = src
        power = np.abs(np.fft.rfft(chunk * window)) ** 2
        low.append(power[lo].sum())
        high.append(power[hi].sum())
    return np.array(low), np.array(high)


def best_lag(video, audio, max_lag):
    """The lag, in frames, at which the two series agree best.

    Positive means the picture moves first. Correlation is on the z-scored
    series so that a loud shot and a bright one are compared on their shape,
    not their level.
    """
    def z(v):
        v = v - v.mean()
        s = v.std()
        return v / s if s else v
    a, b = z(video), z(audio)
    n = len(a)
    lags, scores = [], []
    for lag in range(-max_lag, max_lag + 1):
        if lag >= 0:
            x, y = a[: n - lag], b[lag:]
        else:
            x, y = a[-lag:], b[: n + lag]
        if len(x) < FPS:
            continue
        lags.append(lag)
        scores.append(float(np.dot(x, y) / len(x)))
    i = int(np.argmax(scores))
    return lags[i], scores[i], lags, scores


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('cut')
    ap.add_argument('start', type=float)
    ap.add_argument('dur', type=float)
    ap.add_argument('--crop', default='300:300:810:390',
                    help='W:H:X:Y of the spectrum square in the 1920x1080 frame')
    ap.add_argument('--max-lag', type=float, default=2.0)
    args = ap.parse_args()

    crop = tuple(int(v) for v in args.crop.split(':'))
    vlow, vhigh = video_bands(args.cut, args.start, args.dur, crop)
    alow, ahigh = audio_bands(args.cut, args.start, args.dur, len(vlow))
    max_lag = int(round(args.max_lag * FPS))

    print(f'{len(vlow)} frames, crop {args.crop}, lags +-{args.max_lag:.1f}s')
    print('band          lag_s     r')
    leads = []
    for name, v, a in (('low 40-160Hz', vlow, alow),
                       ('high 1-3.5kHz', vhigh, ahigh)):
        lag, score, _, _ = best_lag(v, a, max_lag)
        leads.append(lag / FPS)
        print(f'{name:<13} {lag / FPS:+6.2f} {score:6.3f}')
    lead = sum(leads) / len(leads)
    print(f'\nmean lead {lead:+.2f} s '
          f'({"picture ahead" if lead > 0 else "sound ahead"})')
    print(f'in-point correction: {args.start:.2f} -> shift the shot by '
          f'{-lead:+.2f} s')


if __name__ == '__main__':
    main()
