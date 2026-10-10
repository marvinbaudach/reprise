"""Late-bound label matchers over the shared CUA vocabulary."""

from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any, Mapping

from search_results import result_elements
from ui_vocabulary import BUTTON_ROLES, ROW_ROLES, canonical_role


@dataclass(frozen=True)
class LabelMatcher:
    exact: tuple[str, ...] = ()
    contains: tuple[str, ...] = ()
    # Regular expressions the whole folded label must match; for a label that
    # carries a count, such as the value "1993 (106)" in a filter popover, where
    # `contains` would also take a track row that happens to hold the digits.
    patterns: tuple[str, ...] = ()
    # Only elements whose centre lies inside an open popup. A popover list is
    # longer than its window, and the tree reports the rows below the fold at the
    # positions they would have; a click there lands on whatever is underneath.
    in_popup: bool = False
    roles: tuple[str, ...] = ()
    require_actionable: bool = True
    require_enabled: bool = True
    strict_roles: bool = False
    # Prefer the page's results over the sidebar rows that share the row role.
    results_only: bool = False

    def candidates(self, observation: Mapping[str, Any]) -> tuple[str, ...]:
        candidates, _mismatch = self.candidates_with_role_fallback(observation)
        return candidates

    def candidates_with_role_fallback(
        self, observation: Mapping[str, Any]
    ) -> tuple[tuple[str, ...], bool]:
        all_candidates = self._matching(observation, use_roles=False)
        if not self.roles:
            return all_candidates, False
        role_candidates = self._matching(observation, use_roles=True)
        if self.strict_roles:
            return role_candidates, bool(all_candidates and not role_candidates)
        return (role_candidates, False) if role_candidates else (all_candidates, bool(all_candidates))

    def resolve(self, observation: Mapping[str, Any]) -> str | None:
        candidates = self.candidates(observation)
        return candidates[0] if candidates else None

    def _matching(
        self, observation: Mapping[str, Any], *, use_roles: bool
    ) -> tuple[str, ...]:
        exact_folded = {value.casefold() for value in self.exact}
        contains_folded = tuple(value.casefold() for value in self.contains)
        patterns = tuple(re.compile(value, re.IGNORECASE) for value in self.patterns)
        popups = [
            popup for popup in observation.get("popups", []) if isinstance(popup, dict)
        ]
        roles = {canonical_role(value) for value in self.roles}
        actionable = set(observation.get("actionable_labels", []))
        # A page whose results cannot be told apart is matched like any other.
        result_ids = (
            {id(item) for item in result_elements(observation)}
            if self.results_only
            else set()
        ) or None
        ranked = []
        for item in observation.get("elements", []):
            if not isinstance(item, dict):
                continue
            label = str(item.get("label") or "")
            folded = label.casefold()
            if not label:
                continue
            if result_ids is not None and id(item) not in result_ids:
                continue
            exact = folded in exact_folded or any(
                pattern.fullmatch(label) for pattern in patterns
            )
            contains = any(value in folded for value in contains_folded)
            if (self.exact or self.contains or self.patterns) and not (exact or contains):
                continue
            if self.in_popup and popups and not _centre_in_any(item, popups):
                continue
            if self.require_actionable and (
                item.get("actionable") is not True or label not in actionable
            ):
                continue
            if self.require_enabled and item.get("enabled") is not True:
                continue
            if use_roles and canonical_role(str(item.get("role", ""))) not in roles:
                continue
            frame = item.get("frame", {})
            y = float(frame.get("y", 0)) if isinstance(frame, dict) else 0.0
            ranked.append((0 if exact else 1, y, label.casefold(), label))
        return tuple(item[-1] for item in sorted(ranked))


def _centre_in_any(item: Mapping[str, Any], rectangles: list[dict[str, Any]]) -> bool:
    frame = item.get("frame")
    if not isinstance(frame, dict):
        return False
    try:
        x = float(frame.get("x", 0)) + float(frame.get("width", frame.get("w", 0))) / 2
        y = float(frame.get("y", 0)) + float(frame.get("height", frame.get("h", 0))) / 2
        return any(
            float(r["x"]) <= x <= float(r["x"]) + float(r["width"])
            and float(r["y"]) <= y <= float(r["y"]) + float(r["height"])
            for r in rectangles
        )
    except (KeyError, TypeError, ValueError):
        return False


BUTTON_MATCHER = LabelMatcher(roles=tuple(sorted(BUTTON_ROLES)))
ROW_MATCHER = LabelMatcher(roles=tuple(sorted(ROW_ROLES)))
