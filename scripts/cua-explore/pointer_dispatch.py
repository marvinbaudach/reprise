#!/usr/bin/env python3
"""Shared cua-driver payloads for the real desktop pointer route."""

from __future__ import annotations

from typing import Any


def desktop_pointer_payload(x: float, y: float) -> dict[str, Any]:
    """Use the schema-valid, session-free global-input route."""
    return {"scope": "desktop", "x": x, "y": y}
