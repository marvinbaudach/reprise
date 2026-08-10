#!/usr/bin/env python3
"""Seeded code-blind explorer that prefers novel, safe GUI states."""

from __future__ import annotations

import hashlib
from typing import Any, Mapping

from protocol import Mission, SCHEMA_VERSION
from ui_vocabulary import canonical_role, hover_strictness


DESTRUCTIVE_WORDS = ("delete", "remove", "forget", "eject", "trash", "erase")
ASYNC_WORDS = ("refresh", "scan", "sync", "download", "analyze", "import", "save", "retry")
# A safety bound against a pathological tree, not the working limit: the
# mission's action budget is what really bounds the sweep.
MAX_HOVER_TARGETS_PER_SECTION = 200
CURRENT_VIEW_SECTION = "(view on entry)"
# Free exploration happens before the workload phase starts; those actions are
# spent either way, so the hover budget has to account for them.
EXPLORATION_PROPOSALS = 12
SURFACE_PRIORITY = (
    "Music",
    "Queue",
    "Playlists",
    "Podcasts",
    "YouTube",
    "Radio",
    "Concerts",
    "Releases",
    "My Stats",
    "Sync",
)


class DeterministicExplorer:
    """Proposes bounded actions from observations without source-code knowledge."""

    def __init__(self, mission: Mission, seed: int) -> None:
        self.mission = mission
        self.seed = seed
        self._tried: set[tuple[str, str, str]] = set()
        self._seen_labels: set[str] = set()
        self._fallback_index = 0
        self._pending_status_probe = False
        self._proposal_count = 0
        self._workload_index = 0
        self._workload_queue: list[dict[str, Any]] = []
        self._hover_section_index = 0
        self._hover_pending_section: str | None = None
        self._hover_seen: set[tuple[str, str]] = set()
        # What the sweep actually covered, so a truncated sweep is visible.
        self.hover_coverage: list[dict[str, Any]] = []
        self._hover_swept_current = False
        self._hover_planned_sections = 0

    def propose(self, observation: Mapping[str, Any]) -> dict[str, Any]:
        self._latest_observation = observation
        state_id = str(observation.get("state_id", ""))
        self._proposal_count += 1
        if self._pending_status_probe:
            self._pending_status_probe = False
            return {
                "schema_version": SCHEMA_VERSION,
                "state_id": state_id,
                "kind": "wait",
                "duration_ms": 1_000,
                "expect_status": True,
            }
        if self._proposal_count >= EXPLORATION_PROPOSALS:
            workload_action = self._next_workload_action(state_id, observation)
            if workload_action is not None:
                return workload_action
        signature = str(observation.get("state_signature", state_id))
        labels_raw = observation.get("actionable_labels", [])
        labels = [str(label) for label in labels_raw if isinstance(label, str)]
        safe_labels = [label for label in labels if self._safe_label(label)]

        action = self._novel_surface_action(state_id, signature, safe_labels)
        if action is None:
            action = self._novel_label_action(state_id, signature, safe_labels)
        if action is None:
            action = self._search_action(state_id, signature, safe_labels)
        if action is None:
            action = self._fallback_action(state_id, observation)
        return action

    def _safe_label(self, label: str) -> bool:
        folded = label.casefold()
        return not any(word in folded for word in DESTRUCTIVE_WORDS)

    def _novel_surface_action(
        self, state_id: str, signature: str, labels: list[str]
    ) -> dict[str, Any] | None:
        for label in SURFACE_PRIORITY:
            key = signature, "activate", label
            if label in labels and key not in self._tried:
                self._tried.add(key)
                self._seen_labels.add(label)
                return self._activate(state_id, label)
        return None

    def _novel_label_action(
        self, state_id: str, signature: str, labels: list[str]
    ) -> dict[str, Any] | None:
        candidates = [
            label
            for label in labels
            if label != "Search all fields"
            and (signature, "activate", label) not in self._tried
        ]
        if not candidates:
            return None
        candidates.sort(key=lambda label: (label in self._seen_labels, self._rank(label)))
        label = candidates[0]
        self._tried.add((signature, "activate", label))
        self._seen_labels.add(label)
        return self._activate(state_id, label)

    def _search_action(
        self, state_id: str, signature: str, labels: list[str]
    ) -> dict[str, Any] | None:
        label = "Search all fields"
        if label not in labels or "type" not in self.mission.capabilities:
            return None
        token = "SEARCH_NEEDLE"
        if token not in self.mission.fixture_tokens:
            return None
        key = signature, "type", token
        if key in self._tried:
            return None
        self._tried.add(key)
        return {
            "schema_version": SCHEMA_VERSION,
            "state_id": state_id,
            "kind": "type",
            "target": {"label": label},
            "fixture_token": token,
        }

    def _fallback_action(
        self, state_id: str, observation: Mapping[str, Any] | None = None
    ) -> dict[str, Any]:
        fallbacks = []
        if "scroll" in self.mission.capabilities:
            fallbacks.extend(
                [
                    {"kind": "scroll", "direction": "down", "amount": 1, "by": "page"},
                    {"kind": "scroll", "direction": "up", "amount": 1, "by": "page"},
                ]
            )
        if "resize" in self.mission.capabilities:
            fallbacks.extend(
                [
                    {"kind": "resize", "width": 720, "height": 760},
                    {"kind": "resize", "width": 1200, "height": 800},
                ]
            )
        if "wait" in self.mission.capabilities:
            fallbacks.append(
                {"kind": "wait", "duration_ms": 750, "expect_status": False}
            )
        if not fallbacks:
            return {
                "schema_version": SCHEMA_VERSION,
                "state_id": state_id,
                "kind": "finish",
                "reason": "No safe novel action remains",
            }
        if self._fallback_index >= len(fallbacks) * 3:
            workload_action = self._next_workload_action(state_id, observation or {})
            if workload_action is not None:
                return workload_action
        action = dict(fallbacks[self._fallback_index % len(fallbacks)])
        self._fallback_index += 1
        action.update({"schema_version": SCHEMA_VERSION, "state_id": state_id})
        return action

    def _next_workload_action(
        self, state_id: str, observation: Mapping[str, Any]
    ) -> dict[str, Any] | None:
        if not self._workload_queue and self._workload_index < len(self.mission.workloads):
            workload = self.mission.workloads[self._workload_index]
            kind = workload.get("kind")
            if kind == "scroll-sweep":
                pages = int(workload.get("pages", 1))
                for direction in workload.get("directions", []):
                    remaining = pages
                    while remaining:
                        amount = min(10, remaining)
                        self._workload_queue.append(
                            {
                                "kind": "scroll",
                                "direction": direction,
                                "amount": amount,
                                "by": "page",
                            }
                        )
                        remaining -= amount
            elif kind == "restart":
                section = workload.get("section")
                if section:
                    self._workload_queue.append(
                        {"kind": "activate", "target": {"label": str(section)}}
                    )
                search_token = workload.get("search_token")
                if search_token:
                    self._workload_queue.append(
                        {
                            "kind": "type",
                            "target": {"label": "Search all fields"},
                            "fixture_token": str(search_token),
                        }
                    )
                self._workload_queue.append(
                    {
                        "kind": "restart",
                        "reason": str(
                            workload.get("reason", "Verify disposable session restoration")
                        ),
                    }
                )
            elif kind == "hover-sweep":
                action = self._next_hover_sweep_action(state_id, observation, workload)
                if action is not None:
                    return action
                self._workload_queue.append(
                    {
                        "kind": "complete-workload",
                        "workload_index": self._workload_index,
                    }
                )
                self._workload_index += 1
            else:
                return None
            if kind != "hover-sweep":
                self._workload_queue.append(
                    {
                        "kind": "complete-workload",
                        "workload_index": self._workload_index,
                    }
                )
                self._workload_index += 1
        if self._workload_queue:
            action = self._workload_queue.pop(0)
            return {
                "schema_version": SCHEMA_VERSION,
                "state_id": state_id,
                **action,
            }
        if self._workload_index == len(self.mission.workloads):
            return {
                "schema_version": SCHEMA_VERSION,
                "state_id": state_id,
                "kind": "finish",
                "reason": "Bounded deterministic workload coverage is complete",
            }
        return None

    def hover_budget_per_section(self, sections: int) -> int:
        """Spread the action budget over the sections that can actually be swept.

        Reserved: the free exploration before the workload starts, one
        activation per section, the workload checkpoint, the finish action, and
        a small margin for recovery. Counting sections that have no accessible
        handle would hand most of the budget to sections that are never visited.
        """
        if sections <= 0:
            return 0
        reserve = sections + 2 + 4 + (EXPLORATION_PROPOSALS - 1)
        available = max(0, int(self.mission.budgets.actions) - reserve)
        return max(1, min(MAX_HOVER_TARGETS_PER_SECTION, available // sections))

    def _next_hover_sweep_action(
        self,
        state_id: str,
        observation: Mapping[str, Any],
        workload: Mapping[str, Any],
    ) -> dict[str, Any] | None:
        sections = tuple(str(item) for item in workload.get("sections", []))
        if self._hover_pending_section is not None:
            # The mission names its roles in AT-SPI spelling, but the driver
            # answers with its own ("push button" for "button"), so matching
            # that list literally produced zero hovers. Anything the hover
            # rulebook has a contract for is a target: buttons and links
            # strictly, rows, cells, tabs, chips and tiles softly.
            extra = {canonical_role(str(item)) for item in workload.get("roles", [])}
            candidates = []
            without_geometry = 0
            for item in observation.get("elements", []):
                if not isinstance(item, dict):
                    continue
                label = str(item.get("label") or "")
                role = canonical_role(str(item.get("role", "")))
                if (
                    not label
                    or item.get("actionable") is not True
                    or item.get("enabled") is not True
                    or item.get("visible") is not True
                    or (hover_strictness(role) == "skip" and role not in extra)
                    or (self._hover_pending_section, label) in self._hover_seen
                ):
                    continue
                if item.get("geometry_trusted") is False:
                    # Its frame is the driver's placeholder, so hovering it
                    # would measure some other part of the window.
                    without_geometry += 1
                    continue
                frame = item.get("frame", {})
                if not isinstance(frame, dict):
                    frame = {}
                candidates.append(
                    (
                        float(frame.get("y", 0)),
                        float(frame.get("x", 0)),
                        label,
                    )
                )
            limit = self.hover_budget_per_section(
                self._hover_planned_sections or len(sections) + 1
            )
            # One visit per label: two elements sharing a label would otherwise
            # spend two of the section's slots on the same measurement.
            ordered = []
            taken: set[str] = set()
            for entry in sorted(candidates):
                if entry[2] in taken:
                    continue
                taken.add(entry[2])
                ordered.append(entry)
            selected = ordered[:limit]
            for _y, _x, label in selected:
                self._hover_seen.add((self._hover_pending_section, label))
                self._workload_queue.append(
                    {"kind": "hover", "target": {"label": label}}
                )
            self.hover_coverage.append(
                {
                    "section": self._hover_pending_section,
                    "reachable": True,
                    "candidates": len(ordered),
                    "hovered": len(selected),
                    "skipped_budget": len(ordered) - len(selected),
                    "skipped_without_geometry": without_geometry,
                    "limit_per_section": limit,
                    "planned_sections": self._hover_planned_sections,
                }
            )
            self._hover_pending_section = None
            if self._workload_queue:
                action = self._workload_queue.pop(0)
                return {"schema_version": SCHEMA_VERSION, "state_id": state_id, **action}
        # Sweep whatever is on screen first. The sidebar sections are not
        # exposed to accessibility at all, so a sweep that only visits them
        # measures nothing - and used to abort the whole run.
        if not self._hover_swept_current:
            self._hover_swept_current = True
            # Only sections with an accessible handle will ever be swept, so
            # only those get a share of the budget.
            offered = {str(label) for label in observation.get("actionable_labels", [])}
            self._hover_planned_sections = 1 + sum(
                1 for section in sections if section in offered
            )
            self._hover_pending_section = CURRENT_VIEW_SECTION
            return self._next_hover_sweep_action(state_id, observation, workload)
        reachable = {
            str(label) for label in observation.get("actionable_labels", [])
        }
        while self._hover_section_index < len(sections):
            section = sections[self._hover_section_index]
            self._hover_section_index += 1
            if section in reachable:
                self._hover_pending_section = section
                return self._activate(state_id, section)
            # Not a harness failure: the section simply has no accessible
            # handle, which is itself worth reporting.
            self.hover_coverage.append(
                {
                    "section": section,
                    "reachable": False,
                    "candidates": 0,
                    "hovered": 0,
                    "skipped_budget": 0,
                    "skipped_without_geometry": 0,
                    "limit_per_section": 0,
                }
            )
        return None

    def _activate(self, state_id: str, label: str) -> dict[str, Any]:
        def _invocable_actions_local(names: object) -> tuple[str, ...]:
            structural = ("listitem.", "list.", "win.", "window.", "default.")
            if not isinstance(names, (list, tuple)):
                return ()
            return tuple(
                str(name)
                for name in names
                if isinstance(name, str) and not name.startswith(structural)
            )

        if any(word in label.casefold() for word in ASYNC_WORDS):
            self._pending_status_probe = True
        dispatch = "px" if self.mission.mission_id == "pointer-layout-reachability" else "ax"
        elements = getattr(self, "_latest_observation", {}).get("elements", [])
        matches = [
            item
            for item in elements
            if isinstance(item, dict) and item.get("label") == label
        ]
        target = next(
            (
                item
                for item in matches
                if _invocable_actions_local(item.get("actions"))
            ),
            matches[0] if matches else {},
        )
        if "actions" in target and not _invocable_actions_local(target.get("actions")):
            frame = target.get("frame", {})
            width = frame.get("width", frame.get("w", 0)) if isinstance(frame, dict) else 0
            height = frame.get("height", frame.get("h", 0)) if isinstance(frame, dict) else 0
            if target.get("geometry_trusted") is not False and width > 0 and height > 0:
                dispatch = "px"
        return {
            "schema_version": SCHEMA_VERSION,
            "state_id": state_id,
            "kind": "activate",
            "target": {"label": label},
            "dispatch": dispatch,
            "expect_effect": "required",
        }

    def _rank(self, label: str) -> str:
        payload = f"{self.seed}:{label}".encode("utf-8")
        return hashlib.sha256(payload).hexdigest()
