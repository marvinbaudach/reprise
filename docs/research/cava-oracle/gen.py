#!/usr/bin/env python3
"""Writes the synthetic test signal as little-endian f32 mono PCM.

usage: gen.py out.f32 [frames]     (default 360 frames of 735 samples)

The formula is the one `ac_28_cava_bars_match_the_cavacore_reference_after_
calibration` in crates/reprise-core/src/playback/cava_tests.rs computes: each
sample is evaluated in f64 and then rounded to f32.
"""
import math
import struct
import sys

HOP = 735
FRAMES = int(sys.argv[2]) if len(sys.argv) > 2 else 360
TAU = 2.0 * math.pi


def sample(n):
    t = n / 44100.0
    kick = math.exp(-(t % 0.5) / 0.06)
    return (
        0.45 * kick * math.sin(TAU * 55.0 * t)
        + 0.15 * math.sin(TAU * 440.0 * t)
        + 0.08 * (0.5 + 0.5 * math.sin(TAU * 1.5 * t)) * math.sin(TAU * 2500.0 * t)
        + 0.04 * math.sin(TAU * 7000.0 * t)
    )


with open(sys.argv[1], "wb") as out:
    for n in range(FRAMES * HOP):
        out.write(struct.pack("<f", sample(n)))
