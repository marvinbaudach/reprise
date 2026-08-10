"""Late-bound label matchers over the shared CUA vocabulary."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Mapping

from ui_vocabulary import BUTTON_ROLES, ROW_ROLES, canonical_role


@dataclass(frozen=True)
class LabelMatcher:
    exact: tuple[str, ...] = ()
    contains: tuple[str, ...] = ()
    roles: tuple[str, ...] = ()
    require_actionable: bool = True
    require_enabled: bool = True
    strict_roles: bool = False

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
        roles = {canonical_role(value) for value in self.roles}
        actionable = set(observation.get("actionable_labels", []))
        ranked = []
        for item in observation.get("elements", []):
            if not isinstance(item, dict):
                continue
            label = str(item.get("label") or "")
            folded = label.casefold()
            if not label:
                continue
            exact = folded in exact_folded
            contains = any(value in folded for value in contains_folded)
            if (self.exact or self.contains) and not (exact or contains):
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


BUTTON_MATCHER = LabelMatcher(roles=tuple(sorted(BUTTON_ROLES)))
ROW_MATCHER = LabelMatcher(roles=tuple(sorted(ROW_ROLES)))
