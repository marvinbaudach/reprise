#!/usr/bin/env python3
"""Read element geometry from AT-SPI ourselves, because the driver cannot.

Under X11/Xvfb the AT-SPI SCREEN coordinate space reports (0, 0) for every
node - measured on Reprise (170 of 170) and on gnome-calculator (107 of 107),
so it is the environment, not the app. cua-driver asks for SCREEN and adds the
window origin, which lands every element on the same pixel and makes every
position-dependent oracle meaningless.

WINDOW coordinates do carry real offsets. This module walks the tree itself,
normalises against the frame node, and adds the window origin that list_windows
reports. It refuses to hand out geometry it cannot prove belongs to the element.
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from typing import Any, Mapping, Sequence

from ui_vocabulary import WINDOW_ROLES, canonical_role


# The frame node is the CSD window including its shadow border, so children can
# sit at slightly negative WINDOW coordinates. Allow that much slack when
# checking that a rectangle really lands inside the window.
WINDOW_BOUNDS_TOLERANCE_PX = 32.0
SIZE_TOLERANCE_PX = 1.0
MAX_WALK_NODES = 4000
MAX_WALK_DEPTH = 40


class GeometryError(RuntimeError):
    """Element geometry could not be established beyond doubt."""


@dataclass(frozen=True)
class GeometryNode:
    role: str
    label: str
    x: float
    y: float
    width: float
    height: float


# cua-driver and Atspi.get_role_name() do not always spell a role the same way.
# Only equivalences we can name belong here; anything else fails loud, and the
# error names both spellings so the pair can be added from a real run.
ROLE_SYNONYMS = {"push button": "button"}


def _role(role: str) -> str:
    normalized = canonical_role(role)
    return ROLE_SYNONYMS.get(normalized, normalized)


def _key(role: str, label: str, width: float, height: float) -> tuple[Any, ...]:
    return _role(role), label, round(width), round(height)


def normalize_to_frame(nodes: Sequence[GeometryNode]) -> list[GeometryNode]:
    """Express every node relative to the frame node's own origin.

    The frame node reports its shadow border, e.g. (-5, -5); list_windows
    reports the visible window. Anchoring on the frame node makes the two agree
    and reproduces exactly what AT-SPI reports as PARENT coordinates.
    """
    frame = next(
        (item for item in nodes if canonical_role(item.role) in WINDOW_ROLES), None
    )
    if frame is None:
        raise GeometryError("the accessibility walk contains no frame node")
    return [
        GeometryNode(
            role=item.role,
            label=item.label,
            x=item.x - frame.x,
            y=item.y - frame.y,
            width=item.width,
            height=item.height,
        )
        for item in nodes
    ]


def _driver_key(element: Mapping[str, Any]) -> tuple[Any, ...]:
    frame = element.get("frame") or {}
    return _key(
        str(element.get("role") or ""),
        str(element.get("label") or ""),
        float(frame.get("w", frame.get("width", 0)) or 0),
        float(frame.get("h", frame.get("height", 0)) or 0),
    )


def align_driver_geometry(
    elements: Sequence[Mapping[str, Any]],
    nodes: Sequence[GeometryNode],
    origin: Any,
) -> dict[int, tuple[float, float, float, float]]:
    """Map each driver element_index to a screen rectangle, or refuse.

    Both walks enumerate the same accessibility tree in the same pre-order, so
    they are aligned by position - and every pair is then verified on role,
    label, width and height. Any disagreement fails the whole snapshot rather
    than attaching a position to the wrong element.
    """
    if len(elements) != len(nodes):
        raise GeometryError(
            f"node count differs: driver {len(elements)}, walk {len(nodes)}"
        )
    normalized = normalize_to_frame(nodes)
    keys = [_driver_key(element) for element in elements]
    for index, (element, item) in enumerate(zip(elements, normalized)):
        node_key = _key(item.role, item.label, item.width, item.height)
        driver_key = keys[index]
        if node_key[:2] != driver_key[:2] or any(
            abs(node_key[position] - driver_key[position]) > SIZE_TOLERANCE_PX
            for position in (2, 3)
        ):
            raise GeometryError(
                f"driver and walk disagree at node {index}: "
                f"{driver_key!r} against {node_key!r}"
            )
    # Twins that are identical in role, label and size cannot be told apart by
    # anything in the data, so they get no geometry at all.
    ambiguous = {key for key, count in Counter(keys).items() if count > 1}
    frames: dict[int, tuple[float, float, float, float]] = {}
    for index, (element, item) in enumerate(zip(elements, normalized)):
        if keys[index] in ambiguous:
            continue
        rect = (
            origin.x + item.x,
            origin.y + item.y,
            item.width,
            item.height,
        )
        _require_inside(rect, origin, index)
        frames[int(element.get("element_index", index))] = rect
    return frames


def _require_inside(
    rect: tuple[float, float, float, float], origin: Any, index: int
) -> None:
    slack = WINDOW_BOUNDS_TOLERANCE_PX
    if (
        rect[0] < origin.x - slack
        or rect[1] < origin.y - slack
        or rect[0] > origin.x + origin.width + slack
        or rect[1] > origin.y + origin.height + slack
    ):
        raise GeometryError(
            f"node {index} lands outside the window: {rect!r} against "
            f"({origin.x}, {origin.y}, {origin.width}, {origin.height})"
        )


def walk_window_nodes(pid: int) -> list[GeometryNode]:
    """Pre-order walk of one application's accessibility tree in WINDOW coords.

    Deliberately thin: everything worth testing lives in the pure functions
    above. Requires the Atspi GObject bindings on the host.
    """
    try:
        import gi

        gi.require_version("Atspi", "2.0")
        from gi.repository import Atspi
    except (ImportError, ValueError) as error:
        raise GeometryError("the Atspi bindings are unavailable") from error

    Atspi.init()
    desktop = Atspi.get_desktop(0)
    application = None
    for index in range(desktop.get_child_count()):
        candidate = desktop.get_child_at_index(index)
        if candidate is None:
            continue
        try:
            if candidate.get_process_id() == pid:
                application = candidate
                break
        except Exception:  # noqa: BLE001 - a dying peer must not abort the walk
            continue
    if application is None:
        raise GeometryError(f"no accessibility application for pid {pid}")

    nodes: list[GeometryNode] = []

    def visit(node: Any, depth: int) -> None:
        if len(nodes) >= MAX_WALK_NODES or depth > MAX_WALK_DEPTH:
            return
        try:
            role = node.get_role_name()
            label = node.get_name() or ""
            component = node.get_component_iface()
            extents = (
                component.get_extents(Atspi.CoordType.WINDOW)
                if component is not None
                else None
            )
        except Exception as error:  # noqa: BLE001
            raise GeometryError(f"the accessibility walk failed: {error}") from error
        if extents is None:
            raise GeometryError(f"node {len(nodes)} exposes no component interface")
        nodes.append(
            GeometryNode(
                role=role,
                label=label,
                x=float(extents.x),
                y=float(extents.y),
                width=float(extents.width),
                height=float(extents.height),
            )
        )
        try:
            count = node.get_child_count()
        except Exception as error:  # noqa: BLE001
            raise GeometryError(f"the accessibility walk failed: {error}") from error
        for index in range(count):
            child = node.get_child_at_index(index)
            if child is not None:
                visit(child, depth + 1)

    visit(application, 0)
    return nodes
