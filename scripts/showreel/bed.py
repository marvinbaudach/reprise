#!/usr/bin/env python3
"""Synthesise the music bed for the 60.0 s cut.

The bed is generated rather than licensed, for two reasons. The page is public,
so a track has to be ours outright — and the cut is already built on a 100 BPM
grid, which means an arrangement written against that same grid lands on the
picture instead of drifting past it.

Everything here is additive: tones are built from the partials they need, so
brightness is a property of the note rather than of a filter applied later.
That keeps the parts from muddying each other in the middle of the spectrum
where the pad and the bass would otherwise fight.

The arrangement is section-by-section faithful to the picture (drums drop in
and out, the phone half gets its own bright top line), but the picture's own
loudness arc — read off pick-window.py's target_arc() — is also applied
directly as a fader automation over the whole mix. That is the same move the
sidechain duck and the fade-in/out already make: an amplitude envelope is not
a filter, and shaping level to picture is the honest way to make a generated
cue track the edit rather than merely resemble it in spirit.
"""
import sys

import numpy as np
from scipy.signal import butter, fftconvolve, sosfilt

SR = 48_000
BPM = 100
BEAT = 60.0 / BPM          # 0.6 s — the grid every cut boundary sits on
BAR = 4 * BEAT             # 2.4 s
DUR = 60.0                 # 100 beats — the cut, to the frame

# The picture's own section boundaries, read off the shot list in cut-film.sh.
# The arrangement changes where the film changes, so the music is cut to the
# edit rather than laid under it. All of them sit on the BEAT grid.
HOOK_KICK = 3.0             # title card clears, the hard cut lands, pulse starts
RUN = 7.2                   # the feature run — full pattern from here
PULLBACK = 34.2             # the agent shot pulls the kit back, not out
BREAK = 39.0                # the cut to the phone — the low point of the arc
PHONE = 41.4                # phone half starts, full again, brighter register
ENDCARD = 56.4              # end card — thins to a single closing hit
RESOLVE = DUR - 1.2         # last 1.2 s: pad only, letting the chord ring out

N = int(DUR * SR)
T = np.arange(N) / SR

# F minor. Written as frequencies rather than note names because nothing here
# needs to name a note twice.
F2, AB2, C3, DB3, EB3, F3, AB3, C4, DB4, EB4, F4, AB4, C5 = (
    87.31, 103.83, 130.81, 138.59, 155.56, 174.61, 207.65, 261.63, 277.18, 311.13, 349.23, 415.30, 523.25
)
# root, then the voicing the pad holds — 13 chords cover the whole 60 s at
# CHORD_LEN each, cycling the same four colours the 31.8 s cut used.
CHORDS = [
    (F2, (F3, AB3, C4)),        # 0   0.0   intro/hook, tonic under the title
    (F2, (F3, AB3, C4)),        # 1   4.8   the hook drives on
    (DB3 / 2, (DB4, F4, AB4)),  # 2   9.6   the run starts moving
    (AB2, (C4, EB4, AB4)),      # 3   14.4
    (F2, (F3, AB3, C4)),        # 4   19.2
    (DB3 / 2, (DB4, F4, AB4)),  # 5   24.0
    (EB3 / 2, (EB4, F4, AB4)),  # 6   28.8  the run's furthest reach
    (DB3 / 2, (DB4, F4, AB4)),  # 7   33.6  pull-back, the wistful colour returns
    (AB2, (C4, EB4, AB4)),      # 8   38.4  the break lifts into the phone half
    (F2, (F3, AB3, C4)),        # 9   43.2
    (DB3 / 2, (DB4, F4, AB4)),  # 10  48.0
    (EB3 / 2, (EB4, F4, AB4)),  # 11  52.8  brightest before the end card
    (F2, (F3, AB3, C4)),        # 12  57.6  resolves home
]
CHORD_LEN = 2 * BAR        # 4.8 s

# The film's own loudness arc — mirrors target_arc() in pick-window.py exactly.
# If that function's steps change, these have to change with it; a mismatch is
# a measurement of nothing.
ARC_STEPS = [(0.0, 0.35), (3.0, 0.72), (7.2, 1.0), (34.2, 0.75),
             (39.0, 0.32), (41.4, 1.0), (56.4, 0.30), (DUR, 0.22)]


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


def taper(sig, secs=0.015):
    """Force a signal to end at exactly zero. A decay envelope alone doesn't
    guarantee that — lengthening a kick's tau without also shortening its tail
    leaves an audible level at the buffer's edge, and `add()` writing that
    edge into the mix is a click, not a release."""
    n = min(int(secs * SR), len(sig))
    out = sig.copy()
    out[-n:] *= np.linspace(1.0, 0.0, n)
    return out


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
    body = np.sin(phase) * decay(n, 0.30)
    click = band(np.random.default_rng(1).normal(0, 1, n), 900, 6000) * decay(n, 0.004) * 0.6
    return taper((body + click) * strength)


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


def sub_dive(strength=0.5):
    """A bare low sine, falling then settling — used at the break and again at
    the phone hit so both land with weight even where the kit stays silent."""
    n = int(1.1 * SR)
    t = np.arange(n) / SR
    return taper(np.sin(2 * np.pi * np.cumsum(60 - 22 * t / 1.1) / SR) * decay(n, 0.32) * strength)


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

# --- bass: eighth-note ostinato on the chord root ----------------------------
# Full through the run and the phone half; thinned to on-beat root pulses
# through the pull-back and the break (34.2-41.4) and again into the end card
# — never silent, because that stretch is exactly where the arc's own gain
# is already lowest and a second cut would carve a hole under it.
step = BEAT / 2
t = RUN
while t < DUR:
    root, _ = chord_at(t)
    on_beat = abs((t / BEAT) - round(t / BEAT)) < 1e-6
    thinned = (PULLBACK <= t < PHONE) or (ENDCARD <= t < RESOLVE)
    if thinned and not on_beat:
        t += step
        continue
    length = 0.34 if on_beat else 0.20
    n = int(length * SR)
    tone = partials(root, length, (1.0, 0.45, 0.12, 0.05))
    envelope = np.minimum(np.arange(n) / (0.008 * SR), 1.0) * decay(n, 0.11)
    gain = (0.34 if on_beat else 0.20) * (0.65 if thinned else 1.0)
    add(bass, t, tone * envelope * gain)
    t += step

# --- drums --------------------------------------------------------------
# Silent under the title card (0-HOOK_KICK, nothing added below).
# `kick_times` records every kick placed, so the sidechain below can duck
# only where a kick actually lands rather than pumping on a blind beat grid.
kick_times = []


def add_kick(t, strength):
    add(drums, t, kick(strength))
    kick_times.append(t)


# Sparse: one kick per bar from the cut, aligned to HOOK_KICK itself so the
# very first hit lands on the cut rather than a bar-length later. The cut
# itself gets a stinger — kick, sub weight and an open hat together — because
# a single downbeat, averaged over 4.2 s of otherwise-silent drums, reads as
# a hair under the pad even at full strength; the picture needs the pulse to
# announce itself, not merely be present in the mix.
t = HOOK_KICK
while t < RUN:
    if abs(((t - HOOK_KICK) / BAR) - round((t - HOOK_KICK) / BAR)) > 1e-6:
        t += BEAT
        continue
    if t == HOOK_KICK:
        add_kick(t, 1.5)
        add(drums, t, sub_dive(0.7))
        add(drums, t, hat(rng, open_=True) * 1.6)
    else:
        add_kick(t, 1.1)
    t += BEAT

# Full kit: kick every beat, clap on the off-bar, hats in eighths.
t = RUN
while t < PULLBACK:
    add_kick(t, 0.9)
    t += BEAT
t = RUN + BEAT
while t < PULLBACK:
    add(drums, t, clap(rng) * 0.55)
    t += 2 * BEAT
RUN_MID = (RUN + PULLBACK) / 2   # 20.7 — the run opens the hats up from here,
                                  # a timbral build that doesn't touch the kick
                                  # or bass, so it rides on the arc's own
                                  # gentle decline instead of fighting it
t = RUN
while t < PULLBACK:
    eighth = round(t / (BEAT / 2))
    open_ = (eighth % 2 == 0) if t >= RUN_MID else (eighth % 4 == 2)
    add(drums, t, hat(rng, open_=open_))
    t += BEAT / 2

# Pulled back: kick on the downbeat only, a soft off-beat hat for company —
# reduced, not gone.
t = PULLBACK
while t < BREAK:
    beat_idx = round((t - PULLBACK) / BEAT)
    if beat_idx % 4 == 0:
        add_kick(t, 0.6)
    elif beat_idx % 2 == 1:
        add(drums, t, hat(rng, open_=False) * 0.6)
    t += BEAT

# The break itself: the kit drops out, but a bare sub tone marks the cut and
# its answer, with the riser (added to `top`, below) filling the 2.4 s between.
add(drums, BREAK, sub_dive(0.5))
add(drums, PHONE, sub_dive(0.85))

# Full kit again for the phone half.
t = PHONE + BEAT
while t < ENDCARD:
    add_kick(t, 0.9)
    t += BEAT
t = PHONE + BEAT
while t < ENDCARD:
    add(drums, t, clap(rng) * 0.55)
    t += 2 * BEAT
t = PHONE
while t < ENDCARD:
    # Open twice as often as the desktop run's hats — one concrete, audible
    # way the phone half reads brighter rather than merely "the arp is on".
    eighth = round(t / (BEAT / 2))
    add(drums, t, hat(rng, open_=(eighth % 2 == 0)))
    t += BEAT / 2

# End card: one closing hit, then the kit steps aside for the pad.
add_kick(ENDCARD, 0.7)

# --- top: the arp plays over the phone half only, so the platform switch is
# audible as a register change, not just a level change ---------------------
t = PHONE
while t < ENDCARD:
    _, voicing = chord_at(t)
    note = voicing[int(round(t / (BEAT / 4))) % len(voicing)] * 4
    length = 0.20
    n = int(length * SR)
    envelope = np.minimum(np.arange(n) / (0.004 * SR), 1.0) * decay(n, 0.055)
    add(top, t, partials(note, length, (1.0, 0.0, 0.32, 0.15, 0.12)) * envelope * 0.22)
    t += BEAT / 4

# --- transitions -------------------------------------------------------------
add(top, BREAK, taper(riser(PHONE - BREAK) * 3.2))

# --- lift: the end card gets a brief rise in the octave above before the pad
# is left to resolve alone -----------------------------------------------
_, lift_voicing = chord_at(ENDCARD)
n = int(1.6 * SR)
t_l = np.arange(n) / SR
lift_swell = np.sin(np.pi * np.minimum(t_l / 1.4, 1.0))
lift_voice = np.zeros(n)
for f in lift_voicing:
    lift_voice += partials(f * 2, 1.6, (1.0, 0.2, 0.05))
add(top, ENDCARD, lift_voice * lift_swell * 0.20)

# --- sidechain: the kick opens a hole for itself -----------------------
# Only where a kick actually landed, not on a blind beat grid — pumping
# under the sparse and pulled-back sections (where most beats are silent)
# was adding fast, arc-unrelated ripple to the envelope for no musical
# reason. Depth is also shallower than a club sidechain: this is a bed
# under picture, and it only has to make room for the kick, not pump.
duck = np.ones(N)
for t in kick_times:
    i = int(t * SR)
    n = min(int(0.30 * SR), N - i)
    if n > 0:
        duck[i : i + n] = np.minimum(duck[i : i + n], 1.0 - 0.28 * decay(n, 0.085))

# The pad carries most of the weight in this balance, with the kit sitting
# under it rather than on top. Two reasons, one of them audible and one of
# them measured: audibly, a bed under a screen capture has to survive a
# laptop speaker without the kick swallowing everything else; measured, a
# bar-length RMS window sees the kick's own attack-and-decay as fast,
# arc-unrelated ripple no matter how quiet it sits, so the sustained pad and
# bass — which do follow `arc_gain` cleanly — need enough share of the mix
# that their slow rise and fall dominates the loudness curve instead of it.
mix = (pad * 1.65 + bass * 0.9) * duck + drums * 0.5 + top

wet = fftconvolve(mix, plate())[:N]
mix = mix * 0.86 + wet * 0.14

# A gentle stereo spread: the pad and the arp move outward, the low end stays
# centred so the bass survives a phone speaker summing to mono.
side = fftconvolve(pad, plate(0.9))[:N] * 0.10
left, right = mix + side, mix - side

# The picture's own loudness arc, applied as fader automation on top of the
# arrangement's own dynamics — the same move as `duck` and `fade` below, just
# reading the level from the edit instead of from the beat.
arc_gain = np.interp(T, [t for t, _ in ARC_STEPS], [v for _, v in ARC_STEPS])

fade = np.minimum(T / 0.6, 1.0) * np.minimum((DUR - T) / 1.8, 1.0)
stereo = np.stack([left * fade * arc_gain, right * fade * arc_gain], axis=1)
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
