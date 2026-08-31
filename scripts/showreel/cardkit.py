"""The grammar the film's cards are built from.

Both cards — the intro that opens the film and the end card that closes it —
are the same mark on the same ground, moving by the same rules. That is the
point: the film is bracketed by one identity, not by two designs that happen to
share a colour. So the ground, the mark's landing, the light streak, the rising
line and the hairline live here once, and each card is only the score that
calls them.

Everything is composited in Python rather than in a filter chain: the moves are
scale, bloom and masked wipes layered on each other, and expressing that as
nested ffmpeg expressions costs more than it buys. Frames go to ffmpeg as raw
video.
"""
import subprocess

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

W, H, FPS = 1920, 1080, 30
BEAT = 0.6

GROUND = (13, 16, 20)
INK = (234, 242, 241)
MUTED = (140, 155, 158)
TEAL = (79, 219, 212)

LOCKUP = 'data/brand/lockup-horizontal-outlined.svg'
FONT_PATH = '/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf'


def frames_for(duration):
    return int(round(duration * FPS))


def ease_out(t, power=3.0):
    """Decelerating: fast in, settling. The one curve a logo landing needs."""
    return 1.0 - (1.0 - min(max(t, 0.0), 1.0)) ** power


def span(frame, start, length):
    """Progress through an event that starts at `start` seconds."""
    if length <= 0:
        return 1.0
    return min(max((frame / FPS - start) / length, 0.0), 1.0)


def font(size):
    return ImageFont.truetype(FONT_PATH, size)


def render_lockup(work, width):
    """The wordmark is fill="currentColor"; standalone that renders black, which
    is invisible on this ground. Same fix the static cards use."""
    light = f'{work}/lockup-light.svg'
    with open(LOCKUP) as src, open(light, 'w') as dst:
        dst.write(src.read().replace('currentColor', f'#{INK[0]:02X}{INK[1]:02X}{INK[2]:02X}'))
    out = f'{work}/lockup.png'
    # Rendered at three times its final width so every scaled frame is a
    # downsample — an upscaled logo edge is the one artefact a card cannot hide.
    subprocess.run(['rsvg-convert', '-w', str(width * 3), '-o', out, light], check=True)
    return Image.open(out).convert('RGBA')


def ground_plate():
    """Ground, a teal glow off the upper left, and a vignette — all static."""
    y, x = np.mgrid[0:H, 0:W].astype(np.float32)
    glow = np.exp(-(((x - 520) ** 2 + (y - 300) ** 2) / (2 * 620.0**2)))
    plate = np.zeros((H, W, 3), np.float32)
    for i, base in enumerate(GROUND):
        plate[..., i] = base + glow * (TEAL[i] * 0.16)

    radius = np.sqrt(((x - W / 2) / (W / 2)) ** 2 + ((y - H / 2) / (H / 2)) ** 2)
    vignette = np.clip(1.12 - 0.42 * radius**2, 0.0, 1.0)
    return plate * vignette[..., None]


def text_layer_at(layer, text, face, fill, centre_x, centre_y):
    """Centre one line on a point of an existing layer."""
    draw = ImageDraw.Draw(layer)
    box = draw.textbbox((0, 0), text, font=face)
    draw.text((centre_x - box[2] / 2, centre_y - box[3] / 2), text, font=face, fill=(*fill, 255))
    return layer


def text_layer(text, face, fill, centre_y, centre_x=W / 2):
    return text_layer_at(blank_layer(), text, face, fill, centre_x, centre_y)


def blank_layer():
    return Image.new('RGBA', (W, H), (0, 0, 0, 0))


def rule_in(layer, y, centre_x, half_width, tone=0.75, thickness=1):
    """A teal tick drawn into a layer, so it rises with what it marks.

    Thickness is a parameter because the web ladder downscales to 720p: a
    one-pixel tick is a two-thirds-of-a-pixel tick there, which resamples to
    nothing.
    """
    ImageDraw.Draw(layer).rectangle(
        [centre_x - half_width, y, centre_x + half_width, y + thickness - 1],
        fill=(*tuple(int(c * tone) for c in TEAL), 255))
    return layer


def land_mark(canvas, mark_hi, frame, width, centre_y,
              at=0.10, dur=0.40, bloom_dur=0.55, overshoot=0.26):
    """The mark lands, overshooting slightly and settling.

    The bloom is the impact: a blurred copy of the mark screened over it, bright
    on landing and gone four frames later — which is what makes the landing read
    as a hit rather than as a dissolve.
    """
    land = ease_out(span(frame, at, dur))
    if land <= 0:
        return canvas

    scale = 1.0 + overshoot - overshoot * land
    scaled_w = max(2, int(round(width * scale)))
    mark = mark_hi.resize(
        (scaled_w, max(1, round(mark_hi.height * scaled_w / mark_hi.width))),
        Image.LANCZOS)
    sheet = Image.new('RGBA', (W, H), (0, 0, 0, 0))
    sheet.paste(mark, ((W - mark.width) // 2, centre_y - mark.height // 2), mark)

    bloom = 0.85 * (1.0 - ease_out(span(frame, at, bloom_dur), 2.0)) + 0.10
    halo = np.asarray(sheet.filter(ImageFilter.GaussianBlur(22)), np.float32) / 255.0
    body = np.asarray(sheet, np.float32) / 255.0
    alpha = body[..., 3:4] * min(land * 3.0, 1.0)
    canvas = canvas * (1 - alpha) + body[..., :3] * 255.0 * alpha
    return 255.0 - (255.0 - canvas) * (1.0 - halo[..., :3] * halo[..., 3:4] * bloom)


def light_streak(canvas, frame, centre_y, at=0.06, dur=0.34, strength=0.55):
    """A streak crosses the mark as it lands, and leaves with it."""
    sweep = span(frame, at, dur)
    if not 0 < sweep < 1:
        return canvas
    y, x = np.mgrid[0:H, 0:W].astype(np.float32)
    centre = -300 + sweep * (W + 600)
    band = np.exp(-((x - centre) ** 2) / (2 * 90.0**2))
    band *= np.exp(-((y - centre_y) ** 2) / (2 * 150.0**2))
    band *= np.sin(np.pi * sweep) * strength
    return 255.0 - (255.0 - canvas) * (1.0 - band[..., None])


def rise_text(canvas, layer, frame, at, dur, lift):
    """A line rises the last few pixels into its place as it fades in."""
    rise = ease_out(span(frame, at, dur))
    if rise <= 0:
        return canvas
    shifted = layer if rise >= 1 else layer.transform(
        (W, H), Image.AFFINE, (1, 0, 0, 0, 1, -(1 - rise) * lift), Image.BILINEAR)
    arr = np.asarray(shifted, np.float32) / 255.0
    alpha = arr[..., 3:4] * rise
    return canvas * (1 - alpha) + arr[..., :3] * 255.0 * alpha


def hairline(canvas, frame, at, dur, y, half_width=260, tone=0.55, centre_x=W // 2):
    """A rule draws outward from its centre."""
    drawn = ease_out(span(frame, at, dur))
    if drawn <= 0:
        return canvas
    half = int(half_width * drawn)
    if half > 1:
        canvas[y : y + 1, centre_x - half : centre_x + half] = np.array(TEAL, np.float32) * tone
    return canvas


def open_encoder(out, frames):
    return subprocess.Popen(
        ['ffmpeg', '-v', 'error', '-y', '-f', 'rawvideo', '-pix_fmt', 'rgb24',
         '-s', f'{W}x{H}', '-r', str(FPS), '-i', '-',
         '-c:v', 'libx264', '-preset', 'medium', '-crf', '19', '-pix_fmt', 'yuv420p',
         '-frames:v', str(frames), out],
        stdin=subprocess.PIPE)


def close_encoder(encoder):
    encoder.stdin.close()
    if encoder.wait() != 0:
        raise SystemExit('the encoder rejected the frames')
