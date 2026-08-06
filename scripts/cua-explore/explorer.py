#!/usr/bin/env python3
"""Seeded code-blind explorer that prefers novel, safe GUI states."""

from __future__ import annotations

import hashlib
from typing import Any, Mapping

from protocol import Mission, SCHEMA_VERSION
from ui_vocabulary import canonical_role


DESTRUCTIVE_WORDS = ("delete", "remove", "forget", "eject", "trash", "erase")
ASYNC_WORDS = ("refresh", "scan", "sync", "download", "analyze", "import", "save", "retry")
MAX_HOVER_TARGETS_PER_SECTION = 28
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

    def propose(self, observation: Mapping[str, Any]) -> dict[str, Any]:
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
        if self._proposal_count >= 12:
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
            action = self._fallback_action(state_id)
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

    def _fallback_action(self, state_id: str) -> dict[str, Any]:
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
            workload_action = self._next_workload_action(state_id, {})
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

    def _next_hover_sweep_action(
        self,
        state_id: str,
        observation: Mapping[str, Any],
        workload: Mapping[str, Any],
    ) -> dict[str, Any] | None:
        sections = tuple(str(item) for item in workload.get("sections", []))
        if self._hover_pending_section is not None:
            roles = {canonical_role(str(item)) for item in workload.get("roles", [])}
            candidates = []
            for item in observation.get("elements", []):
                if not isinstance(item, dict):
                    continue
                label = str(item.get("label") or "")
                if (
                    not label
                    or item.get("actionable") is not True
                    or item.get("enabled") is not True
                    or item.get("visible") is not True
                    or canonical_role(str(item.get("role", ""))) not in roles
                    or (self._hover_pending_section, label) in self._hover_seen
                ):
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
            for _y, _x, label in sorted(candidates)[:MAX_HOVER_TARGETS_PER_SECTION]:
                self._hover_seen.add((self._hover_pending_section, label))
                self._workload_queue.append(
                    {"kind": "hover", "target": {"label": label}}
                )
            self._hover_pending_section = None
            self._hover_section_index += 1
            if self._workload_queue:
                action = self._workload_queue.pop(0)
                return {"schema_version": SCHEMA_VERSION, "state_id": state_id, **action}
        if self._hover_section_index < len(sections):
            section = sections[self._hover_section_index]
            self._hover_pending_section = section
            return self._activate(state_id, section)
        return None

    def _activate(self, state_id: str, label: str) -> dict[str, Any]:
        if any(word in label.casefold() for word in ASYNC_WORDS):
            self._pending_status_probe = True
        dispatch = (
            "px"
            if self.mission.mission_id == "pointer-layout-reachability"
            else "ax"
        )
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
