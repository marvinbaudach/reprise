#!/usr/bin/env python3
"""Dependency-free PNG decoding and bounded RGB rectangle comparison."""

from __future__ import annotations

import math
import pathlib
import struct
import zlib
from dataclasses import dataclass
from typing import Sequence


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


class UnsupportedImage(ValueError):
    """The screenshot uses a PNG encoding this small reader does not support."""


class UnmeasurableImage(ValueError):
    """The requested comparison rectangle contains no comparable pixels."""


@dataclass(frozen=True)
class Image:
    width: int
    height: int
    pixels: tuple[tuple[int, int, int], ...]


@dataclass(frozen=True)
class ChangeStats:
    changed_pixels: int
    total_pixels: int
    ratio: float
    max_delta: int
    mean_delta: float


def _paeth(left: int, above: int, upper_left: int) -> int:
    prediction = left + above - upper_left
    left_distance = abs(prediction - left)
    above_distance = abs(prediction - above)
    upper_left_distance = abs(prediction - upper_left)
    if left_distance <= above_distance and left_distance <= upper_left_distance:
        return left
    if above_distance <= upper_left_distance:
        return above
    return upper_left


def _defilter(scanlines: bytes, width: int, height: int, channels: int) -> bytes:
    stride = width * channels
    expected = height * (stride + 1)
    if len(scanlines) != expected:
        raise UnsupportedImage("PNG scanline payload has an unexpected size")
    decoded = bytearray()
    previous = bytearray(stride)
    offset = 0
    for _row in range(height):
        filter_kind = scanlines[offset]
        offset += 1
        encoded = scanlines[offset : offset + stride]
        offset += stride
        current = bytearray(stride)
        for index, value in enumerate(encoded):
            left = current[index - channels] if index >= channels else 0
            above = previous[index]
            upper_left = previous[index - channels] if index >= channels else 0
            if filter_kind == 0:
                predictor = 0
            elif filter_kind == 1:
                predictor = left
            elif filter_kind == 2:
                predictor = above
            elif filter_kind == 3:
                predictor = (left + above) // 2
            elif filter_kind == 4:
                predictor = _paeth(left, above, upper_left)
            else:
                raise UnsupportedImage(f"unsupported PNG filter: {filter_kind}")
            current[index] = (value + predictor) & 0xFF
        decoded.extend(current)
        previous = current
    return bytes(decoded)


def read_rgb(path: pathlib.Path | str) -> Image:
    """Read non-interlaced 8-bit RGB/RGBA PNG data and discard alpha."""
    payload = pathlib.Path(path).read_bytes()
    if not payload.startswith(PNG_SIGNATURE):
        raise UnsupportedImage("file is not a PNG image")
    offset = len(PNG_SIGNATURE)
    header: tuple[int, int, int, int, int, int, int] | None = None
    compressed = bytearray()
    while offset + 12 <= len(payload):
        length = struct.unpack_from(">I", payload, offset)[0]
        offset += 4
        kind = payload[offset : offset + 4]
        offset += 4
        end = offset + length
        if end + 4 > len(payload):
            raise UnsupportedImage("PNG chunk is truncated")
        data = payload[offset:end]
        offset = end + 4
        if kind == b"IHDR":
            if length != 13:
                raise UnsupportedImage("PNG header has an invalid size")
            header = struct.unpack(">IIBBBBB", data)
        elif kind == b"IDAT":
            compressed.extend(data)
        elif kind == b"IEND":
            break
    if header is None:
        raise UnsupportedImage("PNG header is missing")
    width, height, bit_depth, color_type, compression, filtering, interlace = header
    if width <= 0 or height <= 0:
        raise UnsupportedImage("PNG dimensions must be positive")
    if bit_depth != 8 or color_type not in {2, 6}:
        raise UnsupportedImage("only 8-bit RGB and RGBA PNG images are supported")
    if compression != 0 or filtering != 0 or interlace != 0:
        raise UnsupportedImage("compressed/interlaced PNG mode is unsupported")
    channels = 4 if color_type == 6 else 3
    try:
        decoded = _defilter(zlib.decompress(bytes(compressed)), width, height, channels)
    except zlib.error as error:
        raise UnsupportedImage("PNG image data cannot be decompressed") from error
    pixels = tuple(
        (decoded[index], decoded[index + 1], decoded[index + 2])
        for index in range(0, len(decoded), channels)
    )
    return Image(width=width, height=height, pixels=pixels)


def _bounds(
    rect: Sequence[float], width: int, height: int
) -> tuple[int, int, int, int]:
    if len(rect) != 4:
        raise UnmeasurableImage("comparison rectangle must contain x, y, width, height")
    x, y, rect_width, rect_height = (float(value) for value in rect)
    if rect_width <= 0 or rect_height <= 0:
        raise UnmeasurableImage("comparison rectangle has no area")
    left = max(0, math.floor(x))
    top = max(0, math.floor(y))
    right = min(width, math.ceil(x + rect_width))
    bottom = min(height, math.ceil(y + rect_height))
    if left >= right or top >= bottom:
        raise UnmeasurableImage("comparison rectangle lies outside the image")
    return left, top, right, bottom


def rect_change_ratio(
    before: Image,
    after: Image,
    rect: Sequence[float],
    *,
    channel_delta: int,
    exclude: Sequence[float] | None = None,
) -> ChangeStats:
    """Measure pixels whose largest RGB-channel change reaches the threshold."""
    if (before.width, before.height) != (after.width, after.height):
        raise UnmeasurableImage("screenshots have different dimensions")
    if channel_delta < 0:
        raise ValueError("channel_delta must not be negative")
    left, top, right, bottom = _bounds(rect, before.width, before.height)
    excluded = None
    if exclude is not None:
        try:
            excluded = _bounds(exclude, before.width, before.height)
        except UnmeasurableImage:
            excluded = None
    deltas = []
    for y in range(top, bottom):
        for x in range(left, right):
            if excluded is not None:
                ex_left, ex_top, ex_right, ex_bottom = excluded
                if ex_left <= x < ex_right and ex_top <= y < ex_bottom:
                    continue
            index = y * before.width + x
            deltas.append(
                max(
                    abs(before.pixels[index][channel] - after.pixels[index][channel])
                    for channel in range(3)
                )
            )
    if not deltas:
        raise UnmeasurableImage("cursor exclusion removed every comparable pixel")
    changed = sum(delta >= channel_delta for delta in deltas)
    return ChangeStats(
        changed_pixels=changed,
        total_pixels=len(deltas),
        ratio=changed / len(deltas),
        max_delta=max(deltas),
        mean_delta=sum(deltas) / len(deltas),
    )
