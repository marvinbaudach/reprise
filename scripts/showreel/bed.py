#!/usr/bin/env python3
"""Synthesise the music bed for the 34.8 s cut.

The bed is generated rather than licensed, for two reasons. The page is public,
so a track has to be ours outright — and the cut is already built on a 100 BPM
grid, which means an arrangement written against that same grid lands on the
picture instead of drifting past it.

Everything here is additive: tones are built from the partials they need, so
brightness is a property of the note rather than of a filter applied later.
That keeps the parts from muddying each other in the middle of the spectrum
where the pad and the bass would otherwise fight.
"""
import sys

import numpy as np
from scipy.signal import butter, fftconvolve, sosfilt

SR = 48_000
BPM = 100
BEAT = 60.0 / BPM          # 0.6 s — the grid every cut boundary sits on
BAR = 4 * BEAT             # 2.4 s
DUR = 31.8                 # 53 beats — the cut, to the frame

# The picture's own section boundaries, read off the shot list in cut-film.sh.
# The arrangement changes where the film changes, so the music is cut to the
# edit rather than laid under it.
HOOK_KICK = 3.0            # the statement clears, the pulse starts
RUN = 6.0                  # the feature run
TURN = 16.8                # the dive into the visualiser — drums thin out
DIP = 19.2                 # the phone stands alone after the slide
TAIL = 28.2                # end card

N = int(DUR * SR)
T = np.arange(N) / SR

# F minor. Written as frequencies rather than note names because nothing here
# needs to name a note twice.
F2, AB2, C3, DB3, EB3, F3, AB3, C4, DB4, EB4, F4, AB4, C5 = (
    87.31, 103.83, 130.81, 138.59, 155.56, 174.61, 207.65, 261.63, 277.18, 311.13, 349.23, 415.30, 523.25
)
# root, then the voicing the pad holds
CHORDS = [
    (F2, (F3, AB3, C4)),
    (F2, (F3, AB3, C4)),
    (DB3 / 2, (DB4, F4, AB4)),
    (AB2, (C4, EB4, AB4)),
    (F2, (F3, AB3, C4)),
    (DB3 / 2, (DB4, F4, AB4)),
    (EB3 / 2, (EB4, F4, AB4)),
    (F2, (F3, AB3, C4)),
]
CHORD_LEN = 2 * BAR        # 4.8 s


def chord_at(t):
    return CHORDS[min(int(t / CHORD_LEN), len(CHORDS) - 1)]


def add(buf, start, sig):
    """Mix a signal in at a start time, clipped to the buffer."""
    i = int(start * SR)
    if i >= len(buf):
        return
    end = min(i + len(sig), len(buf))
    buf[i:end] += sig[: end - i]


def decay(n, tau):
    return np.exp(-np.arange(n) / (tau * SR))


def partials(freq, secs, weights, detune=0.0):
    """A tone as the sum of the partials it needs — no filter afterwards."""
    n = int(secs * SR)
    t = np.arange(n) / SR
    out = np.zeros(n)
    for k, w in enumerate(weights, start=1):
        if w == 0.0:
            continue
        f = freq * k
        if f > SR / 2.2:
            break
        out += w * np.sin(2 * np.pi * f * t)
        if detune:
            out += w * 0.6 * np.sin(2 * np.pi * f * (1 + detune) * t + k)
    return out


def kick(strength=1.0):
    n = int(0.42 * SR)
    t = np.arange(n) / SR
    # The pitch envelope is the whole character: 132 Hz down to 46 Hz in 55 ms.
    f = 46 + 86 * np.exp(-t / 0.055)
    phase = 2 * np.pi * np.cumsum(f) / SR
    body = np.sin(phase) * decay(n, 0.10)
    click = band(np.random.default_rng(1).normal(0, 1, n), 900, 6000) * decay(n, 0.004) * 0.6
    return (body + click) * strength


def band(noise, low, high):
    """Percussion has to be band-limited, not merely bright. Differencing white
    noise tilts the whole spectrum up and the hiss then sits on top of the mix
    instead of inside it — measured at a 13 kHz spectral centroid before this."""
    sos = butter(4, [low / (SR / 2), min(high, SR / 2 - 100) / (SR / 2)], btype='band', output='sos')
    return sosfilt(sos, noise)


def clap(rng):
    n = int(0.28 * SR)
    noise = band(rng.normal(0, 1, n), 1200, 4200)
    # Three offset bursts read as a room rather than a single slap.
    out = np.zeros(n)
    for offset, gain in ((0.000, 0.6), (0.011, 0.9), (0.026, 1.0)):
        i = int(offset * SR)
        out[i:] += noise[: n - i] * decay(n - i, 0.055) * gain
    return out * 0.5


def hat(rng, open_=False):
    n = int((0.16 if open_ else 0.055) * SR)
    noise = band(rng.normal(0, 1, n), 5500, 10500)
    return noise * decay(n, 0.05 if open_ else 0.012) * (0.10 if open_ else 0.085)


def riser(secs):
    n = int(secs * SR)
    t = np.arange(n) / SR
    ramp = (t / secs) ** 2
    noise = band(np.random.default_rng(7).normal(0, 1, n), 700, 5200) * ramp * 0.16
    sweep = np.sin(2 * np.pi * np.cumsum(220 + 900 * ramp) / SR) * ramp * 0.06
    return noise + sweep


def plate(secs=1.6):
    """A short decaying-noise impulse response. Not a room — just enough tail
    to keep the pad from ending on a hard edge."""
    n = int(secs * SR)
    rng = np.random.default_rng(11)
    ir = rng.normal(0, 1, n) * np.exp(-np.arange(n) / (0.32 * SR))
    ir[: int(0.012 * SR)] = 0.0
    return ir / np.abs(ir).sum() * 12.0


pad = np.zeros(N)
bass = np.zeros(N)
drums = np.zeros(N)
top = np.zeros(N)
rng = np.random.default_rng(3)

# --- pad: one sustained voicing per chord, overlapping so changes glide ------
for i in range(len(CHORDS)):
    start = i * CHORD_LEN
    if start >= DUR:
        break
    _, voicing = CHORDS[i]
    length = min(CHORD_LEN + 1.4, DUR - start + 1.4)
    n = int(length * SR)
    t = np.arange(n) / SR
    swell = np.minimum(t / 0.9, 1.0) * np.exp(-np.maximum(t - CHORD_LEN, 0) / 0.5)
    voice = np.zeros(n)
    for f in voicing:
        voice += partials(f, length, (1.0, 0.32, 0.16, 0.06, 0.03), detune=0.0016)
    add(pad, start, voice * swell / (len(voicing) * 2.6))

# --- bass: eighth-note ostinato on the chord root ---------------------------
step = BEAT / 2
t = RUN
while t < DUR:
    if TURN - 0.6 <= t < DIP or t >= TAIL:
        t += step
        continue
    root, _ = chord_at(t)
    on_beat = abs((t / BEAT) - round(t / BEAT)) < 1e-6
    length = 0.34 if on_beat else 0.20
    n = int(length * SR)
    tone = partials(root, length, (1.0, 0.45, 0.12, 0.05))
    envelope = np.minimum(np.arange(n) / (0.008 * SR), 1.0) * decay(n, 0.11)
    add(bass, t, tone * envelope * (0.34 if on_beat else 0.20))
    t += step

# --- drums ------------------------------------------------------------------
t = HOOK_KICK
while t < DUR:
    quiet_turn = TURN + 1.2 <= t < DIP          # the statement gets air
    if t >= TAIL or quiet_turn:
        t += BEAT
        continue
    sparse = t < RUN                            # the hook takes one hit a bar
    if sparse and abs((t / BAR) - round(t / BAR)) > 1e-6:
        t += BEAT
        continue
    add(drums, t, kick(0.9 if t >= RUN else 0.7))
    t += BEAT

t = RUN + BEAT
while t < TAIL:
    if not (TURN + 1.2 <= t < DIP):
        add(drums, t, clap(rng) * 0.55)
    t += 2 * BEAT

t = RUN
while t < TAIL:
    if not (TURN + 1.2 <= t < DIP):
        eighth = round(t / (BEAT / 2))
        add(drums, t, hat(rng, open_=(eighth % 4 == 2)))
    t += BEAT / 2

# --- top: an arp over the phone half only, so the platform switch is audible -
t = DIP
while t < TAIL:
    _, voicing = chord_at(t)
    note = voicing[int(round(t / (BEAT / 4))) % len(voicing)] * 2
    length = 0.20
    n = int(length * SR)
    envelope = np.minimum(np.arange(n) / (0.004 * SR), 1.0) * decay(n, 0.055)
    add(top, t, partials(note, length, (1.0, 0.0, 0.22, 0.0, 0.08)) * envelope * 0.085)
    t += BEAT / 4

# --- transitions ------------------------------------------------------------
add(top, TURN, riser(DIP - TURN))
for hit in (0.0, DIP):
    n = int(1.1 * SR)
    tt = np.arange(n) / SR
    add(drums, hit, np.sin(2 * np.pi * np.cumsum(60 - 22 * tt / 1.1) / SR) * decay(n, 0.32) * 0.5)

# --- sidechain: the kick opens a hole for itself ----------------------------
duck = np.ones(N)
t = HOOK_KICK
while t < TAIL:
    i = int(t * SR)
    n = min(int(0.30 * SR), N - i)
    if n > 0:
        duck[i : i + n] = np.minimum(duck[i : i + n], 1.0 - 0.42 * decay(n, 0.085))
    t += BEAT

mix = (pad * 0.55 + bass * 0.9) * duck + drums * 0.9 + top

wet = fftconvolve(mix, plate())[:N]
mix = mix * 0.86 + wet * 0.14

# A gentle stereo spread: the pad and the arp move outward, the low end stays
# centred so the bass survives a phone speaker summing to mono.
side = fftconvolve(pad, plate(0.9))[:N] * 0.10
left, right = mix + side, mix - side

fade = np.minimum(T / 0.6, 1.0) * np.minimum((DUR - T) / 1.8, 1.0)
stereo = np.stack([left * fade, right * fade], axis=1)
peak = np.abs(stereo).max()
stereo = stereo / peak * 0.89 if peak > 0 else stereo

out = sys.argv[1] if len(sys.argv) > 1 else 'bed.wav'
raw = (np.clip(stereo, -1.0, 1.0) * 32767).astype('<i2')
import wave
with wave.open(out, 'wb') as w:
    w.setnchannels(2)
    w.setsampwidth(2)
    w.setframerate(SR)
    w.writeframes(raw.tobytes())
print(f'{out}  {DUR:.3f}s  peak={peak:.3f}')
