"""Tiny deterministic PNG writer for dependency-free CUA image tests."""

from __future__ import annotations

import pathlib
import struct
import zlib
from typing import Sequence


def _chunk(kind: bytes, payload: bytes) -> bytes:
    body = kind + payload
    return struct.pack(">I", len(payload)) + body + struct.pack(">I", zlib.crc32(body))


def write_png(
    path: pathlib.Path,
    width: int,
    height: int,
    pixels: Sequence[Sequence[tuple[int, ...]]],
    *,
    color_type: int = 2,
    bit_depth: int = 8,
    interlace: int = 0,
) -> None:
    channels = 4 if color_type == 6 else 3
    rows = bytearray()
    for row in pixels:
        rows.append(0)
        for pixel in row:
            if len(pixel) != channels:
                raise ValueError("pixel channel count differs from the PNG color type")
            for channel in pixel:
                if bit_depth == 8:
                    rows.append(channel)
                else:
                    rows.extend(struct.pack(">H", channel))
    header = struct.pack(">IIBBBBB", width, height, bit_depth, color_type, 0, 0, interlace)
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + _chunk(b"IHDR", header)
        + _chunk(b"IDAT", zlib.compress(bytes(rows)))
        + _chunk(b"IEND", b"")
    )
