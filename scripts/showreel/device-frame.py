#!/usr/bin/env python3
"""Render the phone the Android footage is shown inside.

Drawn, not downloaded. A stock mockup carries a licence and usually a brand,
and neither belongs in a film about someone else's software. This one is ours
outright and can be sized to whatever the cut needs.

The output is one RGBA image: an opaque body with a transparent hole where the
screen goes. The cut lays the recording underneath and this over it, so the
hole reveals the screen and the body covers the square corners of the video —
one overlay, no separate mask to keep in step.

The corners are superellipse arcs, not circular ones. The difference is the
whole character of a modern handset: a circular corner meets the straight edge
at a visible seam, a superellipse corner curves continuously into it, and the
eye reads that as machined rather than drawn. The edges themselves stay
straight — the exponent acts on the corners alone.

Usage: device-frame.py OUT.png [SCREEN_W SCREEN_H]
"""
import sys

import numpy as np
from PIL import Image

BEZEL = 11              # body edge around the screen, at a ~440 px screen
RIM = 2.0
BODY_N = 5.2            # superellipse exponent for the body
SCREEN_N = 4.6          # the screen's corners are a touch rounder
SHADOW_PAD = 54
BODY_RADIUS = 62
SCREEN_RADIUS = 52

BODY = np.array([20.0, 23.0, 28.0])
RIM_LIGHT = np.array([150.0, 163.0, 176.0])
RIM_DARK = np.array([6.0, 8.0, 11.0])
TEAL = np.array([79.0, 219.0, 212.0])

CAMERA_R = 7.0          # the centred punch-hole, the one unmistakable front cue
CAMERA_DROP = 26.0      # its centre below the top of the screen


def squircle(shape, centre, half, exponent, radius):
    """A rectangle whose corners are superellipse arcs, 1 inside and 0 outside.

    The exponent has to act on the corners alone. Applied to the whole outline
    — half-axes of the full width and height — it rounds the entire silhouette
    and the result is a bar of soap, not a handset; that was the first version.
    Here the distance is measured from the inner rectangle the corner arcs are
    struck from, so the long edges stay dead straight."""
    height, width = shape
    y, x = np.mgrid[0:height, 0:width].astype(np.float64)
    cx, cy = centre
    ax, ay = half
    dx = np.maximum(np.abs(x - cx) - (ax - radius), 0.0)
    dy = np.maximum(np.abs(y - cy) - (ay - radius), 0.0)
    distance = (dx ** exponent + dy ** exponent) ** (1.0 / exponent)
    return np.clip(radius - distance + 0.5, 0.0, 1.0)


def disc(shape, centre, radius):
    height, width = shape
    y, x = np.mgrid[0:height, 0:width].astype(np.float64)
    return np.clip((radius - np.hypot(x - centre[0], y - centre[1])) + 0.5, 0.0, 1.0)


def render(screen_w, screen_h):
    body_w, body_h = screen_w + 2 * BEZEL, screen_h + 2 * BEZEL
    width, height = body_w + 2 * SHADOW_PAD, body_h + 2 * SHADOW_PAD
    shape = (height, width)
    cx, cy = width / 2.0, height / 2.0

    body_r = min(BODY_RADIUS, body_w / 2.0, body_h / 2.0)
    screen_r = min(SCREEN_RADIUS, screen_w / 2.0, screen_h / 2.0)
    body = squircle(shape, (cx, cy), (body_w / 2.0, body_h / 2.0), BODY_N, body_r)
    inner = squircle(shape, (cx, cy), (body_w / 2.0 - RIM, body_h / 2.0 - RIM), BODY_N,
                     body_r - RIM)
    screen = squircle(shape, (cx, cy), (screen_w / 2.0, screen_h / 2.0), SCREEN_N, screen_r)

    colour = np.repeat(BODY[None, None, :], height, axis=0).repeat(width, axis=1)

    # The rim is where the light lives. A single flat outline reads as a drawn
    # border; a light edge biased to the upper left and a dark one opposite it
    # reads as a chamfer catching a key light.
    edge = np.clip(body - inner, 0.0, 1.0)
    y, x = np.mgrid[0:height, 0:width].astype(np.float64)
    key = np.clip(0.5 - 0.5 * ((x - cx) / (body_w / 2.0) + (y - cy) / (body_h / 2.0)), 0.0, 1.0)
    rim_colour = RIM_DARK[None, None, :] + (RIM_LIGHT - RIM_DARK)[None, None, :] * key[..., None]
    colour = colour * (1 - edge[..., None]) + rim_colour * edge[..., None]

    # A teal edge light down the long sides. It is the film's accent, and it is
    # what stops the slab from reading as any phone in any stock photograph.
    flank = np.clip((np.abs(x - cx) / (body_w / 2.0) - 0.86) / 0.14, 0.0, 1.0) ** 1.4
    flank *= np.clip(1.0 - (np.abs(y - cy) / (body_h / 2.0)) ** 2.0, 0.0, 1.0)
    colour += TEAL[None, None, :] * (flank * edge * 0.55)[..., None]

    alpha = np.clip(body - screen, 0.0, 1.0)

    # The punch-hole sits back in front of the screen, so it is added to the
    # opaque part after the screen has been cut away.
    camera = disc(shape, (cx, cy - screen_h / 2.0 + CAMERA_DROP), CAMERA_R)
    alpha = np.clip(alpha + camera * screen, 0.0, 1.0)
    colour = colour * (1 - camera[..., None]) + np.array([4.0, 5.0, 7.0])[None, None, :] * camera[..., None]

    # A glass sheen across the screen: barely there, and the one thing that
    # makes the display read as covered rather than open.
    sheen = np.clip(1.0 - np.abs(((x - cx) / body_w + (y - cy) / body_h) * 2.6 + 0.35), 0.0, 1.0) ** 2
    sheen = sheen * screen * 0.022
    colour = colour * (1 - sheen[..., None]) + 255.0 * sheen[..., None]
    alpha = np.clip(alpha + sheen, 0.0, 1.0)

    shell = np.dstack([np.clip(colour, 0, 255), alpha * 255.0]).astype(np.uint8)
    image = Image.fromarray(shell, 'RGBA')

    # Shadow and glow go underneath, and both have to be kept off the display.
    # Blurred from the body they spill inward across the screen hole, and the
    # first version tinted the whole picture teal — the phone looked lit from
    # within and the app's own colours were gone.
    from PIL import ImageFilter

    def blurred(mask, radius, shift=0):
        source = np.roll(mask, shift, axis=0) if shift else mask
        plate = Image.fromarray((source * 255.0).astype(np.uint8), 'L')
        return np.asarray(plate.filter(ImageFilter.GaussianBlur(radius)), np.float64) / 255.0

    outside = np.clip(1.0 - body, 0.0, 1.0)
    cast_a = blurred(body, 30, shift=14) * 0.70 * outside
    glow_a = blurred(body, 46) * 0.30 * outside

    ground = np.zeros((height, width, 3))
    ground_a = np.zeros((height, width))
    ground = ground * (1 - cast_a[..., None])              # the shadow is black
    ground_a = np.clip(ground_a + cast_a, 0.0, 1.0)
    ground = (ground * ground_a[..., None] * (1 - glow_a[..., None])
              + TEAL[None, None, :] * glow_a[..., None])
    ground_a = np.clip(ground_a + glow_a, 0.0, 1.0)

    halo = Image.fromarray(
        np.dstack([np.clip(ground, 0, 255), ground_a * 255.0]).astype(np.uint8), 'RGBA')
    halo.alpha_composite(image)

    return halo, (SHADOW_PAD + BEZEL, SHADOW_PAD + BEZEL)


def main():
    out = sys.argv[1]
    screen_w = int(sys.argv[2]) if len(sys.argv) > 2 else 437
    screen_h = int(sys.argv[3]) if len(sys.argv) > 3 else 906

    shell, (screen_x, screen_y) = render(screen_w, screen_h)
    shell.save(out)
    # The cut needs the hole's position to place the footage. Printing it keeps
    # the geometry in one place instead of duplicating it as constants in a
    # shell script, where the two would drift apart.
    print(f'{shell.width} {shell.height} {screen_x} {screen_y} {screen_w} {screen_h}')


if __name__ == '__main__':
    main()
