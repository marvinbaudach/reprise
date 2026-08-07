#!/usr/bin/env python3
"""Standalone click rig: does the accessibility action do what a real click does?

`suspected-no-handler` cannot tell two very different faults apart, because the
explorer dispatches activations over AT-SPI. Either the control does nothing at
all, or only its accessibility action is unwired while a real pointer click
works. This probe drives the same target both ways and contrasts what each one
changed - the state signature, the pixels inside the element, and the rating
stars, which are individual buttons labelled with a glyph.
"""

from __future__ import annotations

import json
import pathlib
from dataclasses import asdict, dataclass, field
from typing import Any, Callable, Mapping, Sequence

from atspi_geometry import GeometryError, resolve_driver_geometry
from driver import DriverError
from hover_geometry import (
    WindowGeometry,
    element_center,
    frame_values,
    to_screenshot_rect,
)
from hover_oracle import HOVER_MIN_CHANNEL_DELTA
from oracles import normalize_snapshot
from pngdiff import UnmeasurableImage, UnsupportedImage, read_rgb, rect_change_ratio


FILLED_STAR = "★"
EMPTY_STAR = "☆"
# Below this a pixel click is a coin toss against rounding and borders.
MIN_PIXEL_TARGET_PX = 8.0


@dataclass(frozen=True)
class ClickResult:
    route: str
    dispatched: bool
    address: dict[str, Any] = field(default_factory=dict)
    signature_changed: bool = False
    changed_ratio: float | None = None
    stars_before: dict[str, int] = field(default_factory=dict)
    stars_after: dict[str, int] = field(default_factory=dict)
    rect: tuple[float, float, float, float] | None = None
    note: str = ""


def star_counts(elements: Sequence[Mapping[str, Any]]) -> dict[str, int]:
    """Rating stars are individual buttons labelled with a single glyph."""
    filled = empty = 0
    for item in elements:
        if not isinstance(item, Mapping):
            continue
        label = str(item.get("label") or "")
        if label == FILLED_STAR:
            filled += 1
        elif label == EMPTY_STAR:
            empty += 1
    return {"filled": filled, "empty": empty}


def _elements(raw: Mapping[str, Any]) -> list[Mapping[str, Any]]:
    structured = raw.get("structuredContent")
    container = structured if isinstance(structured, dict) else raw
    elements = container.get("elements")
    return [item for item in elements if isinstance(item, Mapping)] if isinstance(elements, list) else []


def _find(raw: Mapping[str, Any], label: str) -> Mapping[str, Any]:
    for item in _elements(raw):
        if item.get("label") == label:
            return item
    raise DriverError(f"click probe target is not on screen: {label}")


def _signature(raw: Mapping[str, Any]) -> Any:
    return normalize_snapshot(raw, state_id="probe", captured_ms=0).state_signature


def _measured_frame(
    raw: Mapping[str, Any],
    element: Mapping[str, Any],
    origin: Any,
    geometry_provider: Callable[[], Any] | None,
) -> tuple[Mapping[str, Any], bool, str]:
    """Prefer our own measurement and say plainly when there is none."""
    if element.get("geometry_trusted") is False:
        return element.get("frame") or {}, False, "this element's position was not measured"
    if geometry_provider is None:
        return element.get("frame") or {}, True, ""
    try:
        resolution = resolve_driver_geometry(
            _elements(raw), geometry_provider(), origin
        )
    except GeometryError as error:
        return element.get("frame") or {}, False, f"measured geometry refused: {error}"
    rect = resolution.frames.get(int(element.get("element_index", -1)))
    if rect is None:
        return (
            element.get("frame") or {},
            False,
            "this element has no unambiguous measured position",
        )
    return {"x": rect[0], "y": rect[1], "w": rect[2], "h": rect[3]}, True, ""


def probe_click(
    transport: Any,
    *,
    pid: int,
    window_id: int,
    session: str,
    origin: WindowGeometry,
    label: str,
    evidence_dir: pathlib.Path,
    geometry_provider: Callable[[], Any] | None = None,
) -> list[ClickResult]:
    """Activate one control over AT-SPI, then over pixels, and compare."""
    evidence_dir.mkdir(parents=True, exist_ok=True)

    def snapshot(stem: str) -> tuple[Mapping[str, Any], pathlib.Path]:
        path = evidence_dir / f"{stem}.png"
        raw = transport.call(
            "get_window_state",
            {
                "pid": pid,
                "window_id": window_id,
                "session": session,
                "screenshot_out_file": str(path),
            },
        )
        return raw, path

    results: list[ClickResult] = []
    for route in ("ax", "px"):
        before_raw, before_path = snapshot(f"click-probe-{route}-before")
        element = _find(before_raw, label)
        frame, measured, note = _measured_frame(
            before_raw, element, origin, geometry_provider
        )
        rect = to_screenshot_rect(frame, origin) if measured else None
        address: dict[str, Any] = {}
        if route == "ax":
            index = element.get("element_index")
            if not isinstance(index, int):
                results.append(
                    ClickResult(
                        route=route,
                        dispatched=False,
                        stars_before=star_counts(_elements(before_raw)),
                        stars_after=star_counts(_elements(before_raw)),
                        note="the driver exposes no element_index for this target",
                    )
                )
                continue
            address = {"element_index": index}
        else:
            if not measured:
                results.append(
                    ClickResult(
                        route=route,
                        dispatched=False,
                        stars_before=star_counts(_elements(before_raw)),
                        stars_after=star_counts(_elements(before_raw)),
                        note=f"pixel route skipped: {note or 'position was not measured'}",
                    )
                )
                continue
            _x, _y, width, height = frame_values(frame)
            if width < MIN_PIXEL_TARGET_PX or height < MIN_PIXEL_TARGET_PX:
                note = (
                    f"{width:.0f}x{height:.0f} is too small for a dependable "
                    "pixel click; read this row with care"
                )
            centre = element_center(frame)
            address = {"x": centre[0], "y": centre[1]}

        before_signature = _signature(before_raw)
        stars_before = star_counts(_elements(before_raw))
        transport.call(
            "click",
            {
                "pid": pid,
                "window_id": window_id,
                "session": session,
                **address,
            },
        )
        after_raw, after_path = snapshot(f"click-probe-{route}-after")
        ratio: float | None = None
        if rect is not None:
            try:
                ratio = rect_change_ratio(
                    read_rgb(before_path),
                    read_rgb(after_path),
                    rect,
                    channel_delta=HOVER_MIN_CHANNEL_DELTA,
                ).ratio
            except (OSError, UnsupportedImage, UnmeasurableImage) as error:
                note = f"{note} pixel comparison failed: {error}".strip()
        results.append(
            ClickResult(
                route=route,
                dispatched=True,
                address=address,
                signature_changed=_signature(after_raw) != before_signature,
                changed_ratio=ratio,
                stars_before=stars_before,
                stars_after=star_counts(_elements(after_raw)),
                rect=rect,
                note=note,
            )
        )
    return results


def render_click_table(results: Sequence[ClickResult]) -> str:
    lines = [
        "click probe: does the accessibility action do what a real click does?",
        "",
        f"{'route':<7} {'dispatched':<11} {'signature':<11} {'changed_ratio':<14} "
        f"{'stars before':<16} {'stars after':<16}",
    ]
    for item in results:
        def stars(counts):
            if not counts:
                return "-"
            return f"filled {counts.get('filled')} empty {counts.get('empty')}"

        ratio = "-" if item.changed_ratio is None else f"{item.changed_ratio:.4f}"
        lines.append(
            f"{item.route:<7} {str(item.dispatched):<11} "
            f"{str(item.signature_changed):<11} {ratio:<14} "
            f"{stars(item.stars_before):<16} {stars(item.stars_after):<16}"
        )
        if item.note:
            lines.append(f"  note: {item.note}")
    lines.extend(
        [
            "",
            "Read it like this: if neither route changes anything, the control",
            "does nothing and that is a product fault. If only the pixel row",
            "changes, the control works but its accessibility action is not",
            "wired - assistive technology is offered an action that goes",
            "nowhere. If only the accessibility row changes, the pixel click",
            "missed, so check the rect and the target size before concluding.",
        ]
    )
    return "\n".join(lines) + "\n"


def write_click_evidence(
    results: Sequence[ClickResult], evidence_dir: pathlib.Path
) -> pathlib.Path:
    path = evidence_dir / "click-probe.json"
    path.write_text(
        json.dumps([asdict(item) for item in results], indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return path
