#!/usr/bin/env python3
"""The film's own loudness shape, as an ffmpeg volume automation.

A generated track comes back at one level. Told to keep its dynamics it puts
holes in the middle of the film; told not to go quiet it returns a single loud
block. Neither is a cue. So the shape is not asked of the generator any more —
it is applied here, which is what a dub stage does anyway: the music ducks
under the title card, opens at the hook, pulls back at the handover and falls
away under the end card.

The curve is `pick-window.py`'s `target_arc`, so the shape a window was chosen
for and the shape it is then given are the same one. DEPTH scales how far the
dips go: 1.0 is the arc as written (about -10 dB at its quietest), 0.0 is no
automation at all.

Usage: arc-gain.py SECONDS [DEPTH]
"""
import importlib.util
import pathlib
import sys

import numpy as np

_pw = pathlib.Path(__file__).with_name('pick-window.py')
_spec = importlib.util.spec_from_file_location('pick_window', _pw)
pick_window = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pick_window)


def expression(seconds, depth):
    """A piecewise-linear `volume` expression over the arc's own breakpoints."""
    steps = pick_window.arc_steps(seconds)
    # Depth pulls every point towards full level, so the automation can be
    # softened without redrawing the curve.
    steps = [(t, 1.0 - depth * (1.0 - v)) for t, v in steps]
    expr = f'{steps[-1][1]:.4f}'
    for (t0, v0), (t1, v1) in reversed(list(zip(steps, steps[1:]))):
        span = t1 - t0
        ramp = f'{v0:.4f}+({v1 - v0:.4f})*(t-{t0:.4f})/{span:.4f}'
        expr = f'if(lt(t,{t1:.4f}),{ramp},{expr})'
    return expr


def main():
    seconds = float(sys.argv[1])
    depth = float(sys.argv[2]) if len(sys.argv) > 2 else 1.0
    if not 0.0 <= depth <= 1.0:
        raise SystemExit('arc-gain.py: depth is a fraction between 0 and 1')
    print(expression(seconds, depth))


if __name__ == '__main__':
    main()
