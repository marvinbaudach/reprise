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


def choose_frame(
    candidates: Sequence[tuple[str, str, float, float]],
    window_size: tuple[float, float] | None,
) -> int:
    """Pick the frame the driver is describing, or refuse.

    cua-driver's element 0 is the frame, not the application node, and the
    application node carries no component interface at all. Starting one level
    too high shifts both pre-order walks against each other.
    """
    frames = [
        index
        for index, (role, _label, _width, _height) in enumerate(candidates)
        if canonical_role(role) in WINDOW_ROLES
    ]
    if not frames:
        raise GeometryError(
            f"the application exposes no frame child ({len(candidates)} children)"
        )
    if len(frames) == 1:
        return frames[0]
    if window_size is None:
        raise GeometryError(
            f"ambiguous: {len(frames)} frames and no window size to match against"
        )
    matching = [
        index
        for index in frames
        if abs(candidates[index][2] - window_size[0]) <= SIZE_TOLERANCE_PX
        and abs(candidates[index][3] - window_size[1]) <= SIZE_TOLERANCE_PX
    ]
    if len(matching) == 1:
        return matching[0]
    if not matching:
        raise GeometryError(
            f"ambiguous: none of {len(frames)} frames matches the window size "
            f"{window_size!r}"
        )
    raise GeometryError(
        f"ambiguous: two frames of the same size match {window_size!r}"
    )


def geometry_calibration(
    nodes: Sequence[GeometryNode], origin: Any
) -> dict[str, Any]:
    """Measure the shadow border instead of assuming it.

    The frame node reports a non-zero WINDOW position (measured: -5, -5), which
    is the offset between the WINDOW coordinate origin and the frame itself.
    Normalising against the frame removes exactly that offset - and it is the
    right thing to do precisely when the frame node and the list_windows entry
    describe the same rectangle. Sizes are the test for that: measured 1200x800
    on both sides. If they ever disagree, the two are different rectangles and
    anchoring one on the other would be meaningless, so the caller refuses.
    """
    frame = next(
        (item for item in nodes if canonical_role(item.role) in WINDOW_ROLES), None
    )
    if frame is None:
        raise GeometryError("the accessibility walk contains no frame node")
    shadow = (-frame.x, -frame.y)
    size_matches = (
        abs(frame.width - origin.width) <= SIZE_TOLERANCE_PX
        and abs(frame.height - origin.height) <= SIZE_TOLERANCE_PX
    )
    return {
        "frame_window_rect": [frame.x, frame.y, frame.width, frame.height],
        "window_rect": [
            float(origin.x),
            float(origin.y),
            float(origin.width),
            float(origin.height),
        ],
        "window_origin_offset": [shadow[0], shadow[1]],
        "size_matches_list_windows": size_matches,
        "consistent": size_matches,
    }


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
# Measured pairs, each taken from a real run by comparing the driver's roles
# against a role histogram of the live accessibility tree. Never guessed: an
# unknown spelling stays unmatched and is reported with both names.
ROLE_SYNONYMS = {
    "push button": "button",
    "frame": "window",
    "grid cell": "table cell",
    "group": "grouping",
    "tree grid": "table",
}


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


UNRESOLVED_SAMPLE_LIMIT = 40
LABEL_SAMPLE_LIMIT = 80


@dataclass(frozen=True)
class GeometryResolution:
    """Per-element geometry, the quota, and why the rest stayed unresolved."""

    frames: dict[int, tuple[float, float, float, float]]
    driver_elements: int
    walk_nodes: int
    resolved_unique: int
    resolved_ordered: int
    unmatched: int
    ambiguous: int
    out_of_window: int
    degenerate: int
    subset_violations: int
    walk_surplus: int
    unresolved: dict[str, list[dict[str, Any]]]
    calibration: dict[str, Any]

    @property
    def resolved(self) -> int:
        return self.resolved_unique + self.resolved_ordered

    @property
    def trusted(self) -> bool:
        return self.resolved > 0

    def as_record(self) -> dict[str, Any]:
        return {
            "driver_elements": self.driver_elements,
            "walk_nodes": self.walk_nodes,
            "resolved": self.resolved,
            "resolved_unique": self.resolved_unique,
            "resolved_ordered": self.resolved_ordered,
            "resolved_ratio": (
                round(self.resolved / self.driver_elements, 4)
                if self.driver_elements
                else 0.0
            ),
            "unmatched": self.unmatched,
            "ambiguous": self.ambiguous,
            "out_of_window": self.out_of_window,
            "degenerate": self.degenerate,
            "subset_violations": self.subset_violations,
            "walk_surplus": self.walk_surplus,
            "unresolved": self.unresolved,
            "calibration": self.calibration,
        }


def _sample(
    unresolved: dict[str, list[dict[str, Any]]],
    reason: str,
    position: int,
    element: Mapping[str, Any],
    width: float,
    height: float,
    candidates: int,
    driver_count: int = 1,
) -> None:
    bucket = unresolved.setdefault(reason, [])
    if len(bucket) >= UNRESOLVED_SAMPLE_LIMIT:
        return
    bucket.append(
        {
            "element_index": int(element.get("element_index", position)),
            "role": str(element.get("role") or ""),
            "label": str(element.get("label") or "")[:LABEL_SAMPLE_LIMIT],
            "width": round(width),
            "height": round(height),
            "reason": reason,
            "candidates": candidates,
            "driver_count": driver_count,
        }
    )


def resolve_driver_geometry(
    elements: Sequence[Mapping[str, Any]],
    nodes: Sequence[GeometryNode],
    origin: Any,
) -> GeometryResolution:
    """Give every driver element its own position, or none.

    cua-driver reports one entry per indexed row - measured 180 against 485
    nodes in the full walk - so the two trees are never the same shape and a
    positional alignment cannot work. Elements are grouped by their own key of
    role, label, width and height instead.

    A group is resolved only when the driver and the walk hold the *same
    number* of nodes for that key; then they are paired in walk order. That is
    sound because both enumerate the same tree in pre-order and the driver's
    elements are a subset of the walk, so equal counts on a subset mean the two
    sets are identical and the k-th of each is the same node. Anything else -
    no candidate, or a different count - leaves that group without geometry
    while every other element keeps its own. Ordered pairings are counted
    separately from single unique matches, because they rest on that subset
    argument rather than on the key alone.
    """
    calibration = geometry_calibration(nodes, origin)
    if not calibration["consistent"]:
        raise GeometryError(
            f"frame size differs from the list_windows entry "
            f"(driver {len(elements)}, walk {len(nodes)}): "
            f"frame {calibration['frame_window_rect']}, window "
            f"{calibration['window_rect']}"
        )
    normalized = normalize_to_frame(nodes)
    index: dict[tuple[Any, ...], list[GeometryNode]] = {}
    for item in normalized:
        index.setdefault(
            _key(item.role, item.label, item.width, item.height), []
        ).append(item)

    groups: dict[tuple[Any, ...], list[tuple[int, Mapping[str, Any], float, float]]] = {}
    unresolved: dict[str, list[dict[str, Any]]] = {}
    degenerate = 0
    for position, element in enumerate(elements):
        if not isinstance(element, Mapping):
            continue
        frame = element.get("frame") or {}
        width = float(frame.get("w", frame.get("width", 0)) or 0)
        height = float(frame.get("h", frame.get("height", 0)) or 0)
        if width <= 1 or height <= 1:
            # The driver only reports a frame "when AT-SPI reports usable
            # bounds"; a virtualised row is not a failed match.
            degenerate += 1
            _sample(unresolved, "degenerate", position, element, width, height, 0)
            continue
        groups.setdefault(_driver_key(element), []).append(
            (position, element, width, height)
        )

    frames: dict[int, tuple[float, float, float, float]] = {}
    unique = ordered = unmatched = ambiguous = out_of_window = 0
    subset_violations = walk_surplus = 0
    for key, members in groups.items():
        candidates = index.get(key, [])
        if not candidates or len(candidates) != len(members):
            reason = "unmatched" if not candidates else "ambiguous"
            if reason == "ambiguous":
                # More driver elements than nodes means the driver is reporting
                # something the walk cannot see - virtualised rows, or rows the
                # driver synthesises. Either way the subset argument behind
                # ordered pairing does not hold for this key, which is exactly
                # why the group stays unresolved.
                if len(members) > len(candidates):
                    subset_violations += len(members)
                else:
                    walk_surplus += len(members)
            for position, element, width, height in members:
                if reason == "unmatched":
                    unmatched += 1
                else:
                    ambiguous += 1
                _sample(
                    unresolved,
                    reason,
                    position,
                    element,
                    width,
                    height,
                    len(candidates),
                    len(members),
                )
            continue
        for (position, element, width, height), node in zip(members, candidates):
            rect = (origin.x + node.x, origin.y + node.y, node.width, node.height)
            if not _inside(rect, origin):
                out_of_window += 1
                _sample(
                    unresolved,
                    "out_of_window",
                    position,
                    element,
                    width,
                    height,
                    len(candidates),
                    len(members),
                )
                continue
            frames[int(element.get("element_index", position))] = rect
            if len(members) == 1:
                unique += 1
            else:
                ordered += 1
    return GeometryResolution(
        frames=frames,
        driver_elements=len(elements),
        walk_nodes=len(nodes),
        resolved_unique=unique,
        resolved_ordered=ordered,
        unmatched=unmatched,
        ambiguous=ambiguous,
        out_of_window=out_of_window,
        degenerate=degenerate,
        subset_violations=subset_violations,
        walk_surplus=walk_surplus,
        unresolved=unresolved,
        calibration=calibration,
    )


def _inside(rect: tuple[float, float, float, float], origin: Any) -> bool:
    slack = WINDOW_BOUNDS_TOLERANCE_PX
    return not (
        rect[0] < origin.x - slack
        or rect[1] < origin.y - slack
        or rect[0] > origin.x + origin.width + slack
        or rect[1] > origin.y + origin.height + slack
    )


def walk_window_nodes(
    pid: int, window_size: tuple[float, float] | None = None
) -> list[GeometryNode]:
    """Pre-order walk of one window's accessibility tree in WINDOW coordinates.

    Starts at the frame child, because that is where cua-driver's element 0
    starts; the application node above it carries no component interface at all
    and would shift both walks against each other by one level.

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

    candidates = []
    children = []
    for index in range(application.get_child_count()):
        child = application.get_child_at_index(index)
        if child is None:
            continue
        try:
            component = child.get_component_iface()
            extents = (
                component.get_extents(Atspi.CoordType.WINDOW)
                if component is not None
                else None
            )
            candidates.append(
                (
                    child.get_role_name(),
                    child.get_name() or "",
                    float(extents.width) if extents is not None else 0.0,
                    float(extents.height) if extents is not None else 0.0,
                )
            )
            children.append(child)
        except Exception as error:  # noqa: BLE001
            raise GeometryError(f"the accessibility walk failed: {error}") from error
    frame = children[choose_frame(candidates, window_size)]

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

    visit(frame, 0)
    return nodes
