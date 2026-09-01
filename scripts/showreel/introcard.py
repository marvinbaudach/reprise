#!/usr/bin/env python3
"""The animated intro card.

The film used to announce its two platforms in the corner of the hook, where
the caption competed with the visualiser for the same three seconds. It says it
here instead — and says the thing the hook never had room for at all: there is
one Rust core under all of this, and four frontends over it.

That is the claim worth opening on. The cross-platform norm is one web view
shipped four times; this is the opposite trade — shared logic underneath, a
native surface on top of it for each place the app actually runs. The four
columns are the argument: the count is visible before a word of it is read.

Five beats at 100 BPM, 3.0 s, 90 frames. The mark lands on beat 0 with exactly
the moves it lands with at the end of the film, which is the whole point of
opening this way: the film is bracketed by one identity.

The four columns arrive on one beat rather than one per beat — they are one
statement, and a column per beat would run past the length of the card. They
are staggered a couple of frames apart so the row lands left to right, which
reads as a count rather than as four things appearing at once.

Unlike the end card this one does not go to black: it hard-cuts into the hook
on the beat, which is what makes the film start rather than fade up twice.
"""
import sys

import numpy as np

import cardkit
from cardkit import FPS, INK, MUTED

DUR = 3.0
FRAMES = cardkit.frames_for(DUR)

HEAD = 'One Rust core. Four native frontends.'

# Where it runs on top, what it is built from underneath — so the row reads as
# four surfaces rather than four libraries. libadwaita and Material 3 are
# spelled out: they are the reason the app looks native on either desktop, and
# "GTK4 · Material" hides exactly that.
COLUMNS = (
    ('GNOME', 'GTK4 · libadwaita'),
    ('Android', 'Material 3'),
    ('Terminal', 'a real CLI'),
    ('Agents', 'an MCP server'),
)

LOCKUP_W = 700
LOCKUP_Y = 392          # centre
RULE_Y = 516
HEAD_Y = 580
TICK_Y = 668
LABEL_Y = 706
SUB_Y = 750

# An inner band rather than the full width: four columns spread to the frame
# edges read as a footer, not as a set.
BAND_W = 1440
TICK_HALF = 34

FADE_UP = 0.30
COLUMN_STAGGER = 0.06


def column_layer(label, sub, centre_x, label_face, sub_face):
    """One frontend, as a single object: its tick, its name, its toolkit.

    Built as one layer so the tick rises with the words it marks instead of
    being painted onto the canvas underneath them.
    """
    layer = cardkit.blank_layer()
    cardkit.rule_in(layer, TICK_Y, centre_x, TICK_HALF, thickness=2)
    cardkit.text_layer_at(layer, label, label_face, INK, centre_x, LABEL_Y)
    cardkit.text_layer_at(layer, sub, sub_face, MUTED, centre_x, SUB_Y)
    return layer


def main():
    work = sys.argv[1]
    out = sys.argv[2]

    lockup_hi = cardkit.render_lockup(work, LOCKUP_W)
    head_layer = cardkit.text_layer(HEAD, cardkit.font(42), INK, HEAD_Y)

    label_face, sub_face = cardkit.font(32), cardkit.font(25)
    step = BAND_W / len(COLUMNS)
    columns = [
        column_layer(label, sub, int((cardkit.W - BAND_W) / 2 + step * (i + 0.5)),
                     label_face, sub_face)
        for i, (label, sub) in enumerate(COLUMNS)
    ]

    plate = cardkit.ground_plate()
    rng = np.random.default_rng(31)

    encoder = cardkit.open_encoder(out, FRAMES)

    for frame in range(FRAMES):
        canvas = plate.copy()

        # Beat 0 — the mark lands, and a light streak crosses it as it does.
        canvas = cardkit.land_mark(canvas, lockup_hi, frame, LOCKUP_W, LOCKUP_Y)
        canvas = cardkit.light_streak(canvas, frame, LOCKUP_Y)

        # Beat 1 — a hairline draws outward, grouping what comes under it.
        canvas = cardkit.hairline(canvas, frame, 0.60, 0.30, RULE_Y)

        # Beat 2 — the claim.
        canvas = cardkit.rise_text(canvas, head_layer, frame, 1.20, 0.35, 14)

        # Beat 3 — the four of them, counted off left to right.
        for i, layer in enumerate(columns):
            canvas = cardkit.rise_text(
                canvas, layer, frame, 1.80 + i * COLUMN_STAGGER, 0.35, 12)

        # Grain, and the opening half-beat up from black.
        canvas += rng.normal(0.0, 1.7, (cardkit.H, cardkit.W, 1))
        canvas *= cardkit.span(frame, 0.0, FADE_UP)

        encoder.stdin.write(np.clip(canvas, 0, 255).astype(np.uint8).tobytes())

    cardkit.close_encoder(encoder)
    print(f'{out}  {DUR:.3f} s  {FRAMES} frames  ({FPS} fps)')


if __name__ == '__main__':
    main()
