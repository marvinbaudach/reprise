#!/usr/bin/env python3
"""The animated end card.

A trailer does not end on a still. It lands the mark on a beat, lets the claim
arrive under it, and gives the address the last word — each on its own beat, so
the ending is felt as an ending rather than as the film running out.

Six beats at 100 BPM, 3.6 s, 108 frames. Every event below sits on a multiple
of 0.6 s, which is the same grid every cut in the film sits on.

The mark, the ground and the moves come from cardkit — the intro card is built
from the same pieces, which is what makes the film open and close on one
identity. This file is only the score.
"""
import sys

import numpy as np

import cardkit
from cardkit import FPS, MUTED, TEAL

DUR = 3.6
FRAMES = cardkit.frames_for(DUR)

SUMMARY = 'Play and organize your music'
LINK = 'github.com/marvinbaudach/reprise'

LOCKUP_W = 760
LOCKUP_Y = 470          # centre
SUMMARY_Y = 622
RULE_Y = 690
LINK_Y = 752


def main():
    work = sys.argv[1]
    out = sys.argv[2]

    lockup_hi = cardkit.render_lockup(work, LOCKUP_W)
    summary_layer = cardkit.text_layer(SUMMARY, cardkit.font(34), MUTED, SUMMARY_Y)
    link_layer = cardkit.text_layer(LINK, cardkit.font(30), TEAL, LINK_Y)
    plate = cardkit.ground_plate()
    rng = np.random.default_rng(17)

    encoder = cardkit.open_encoder(out, FRAMES)

    for frame in range(FRAMES):
        canvas = plate.copy()

        # Beat 0 — the mark lands, and a light streak crosses it as it does.
        canvas = cardkit.land_mark(canvas, lockup_hi, frame, LOCKUP_W, LOCKUP_Y)
        canvas = cardkit.light_streak(canvas, frame, LOCKUP_Y)

        # Beat 1 — the claim rises into place.
        canvas = cardkit.rise_text(canvas, summary_layer, frame, 0.60, 0.35, 14)

        # Beat 2 — a hairline draws outward from the centre.
        canvas = cardkit.hairline(canvas, frame, 1.20, 0.30, RULE_Y)

        # Beat 3 — the address, last and alone.
        canvas = cardkit.rise_text(canvas, link_layer, frame, 1.80, 0.35, 10)

        # Grain, then the last half-beat to black.
        canvas += rng.normal(0.0, 1.7, (cardkit.H, cardkit.W, 1))
        canvas *= 1.0 - cardkit.span(frame, DUR - 0.45, 0.45)

        encoder.stdin.write(np.clip(canvas, 0, 255).astype(np.uint8).tobytes())

    cardkit.close_encoder(encoder)
    print(f'{out}  {DUR:.3f} s  {FRAMES} frames  ({FPS} fps)')


if __name__ == '__main__':
    main()
