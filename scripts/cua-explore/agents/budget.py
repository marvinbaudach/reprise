"""Up-front and running action budgets for deterministic exploration."""

from __future__ import annotations

import math
from dataclasses import dataclass
from typing import Any, Mapping


class BudgetTooSmall(ValueError):
    """A mission cannot fit its mandatory actions and reserved finish."""


@dataclass(frozen=True)
class BudgetPlan:
    total_actions: int
    mandatory_per_workload: tuple[int, ...]
    checkpoint_actions: int
    finish_reserve: int = 1
    recovery_reserve: int = 0
    probe_allowance: int = 0


def mandatory_step_count(workload: Mapping[str, Any]) -> int:
    kind = workload.get("kind")
    if kind == "section-search":
        return 4 * len(workload.get("route_tokens", {})) + len(
            workload.get("unsupported", [])
        )
    if kind == "restart":
        return 3 + int(workload.get("connectivity") is not None)
    if kind == "offline-transition":
        sources = len(workload.get("source_tokens", {}))
        return 2 * sources + 1 + 1 + sources + 1 + 1 + sources
    if kind == "batch-edit":
        return 12
    if kind == "sort-cycle":
        return int(workload.get("repetitions", 0)) + len(workload.get("columns", []))
    if kind == "combined-filter":
        return 2 * len(workload.get("facets", [])) + 2
    if kind == "scroll-sweep":
        pages = int(workload.get("pages", 0))
        return len(workload.get("directions", [])) * math.ceil(pages / 10)
    if kind == "hover-sweep":
        return len(workload.get("sections", [])) * (
            1 + int(workload.get("min_targets_per_section", 0))
        )
    raise BudgetTooSmall(f"unknown workload kind: {kind}")


def plan_budget(mission: Mapping[str, Any]) -> BudgetPlan:
    total = int(mission.get("budgets", {}).get("actions", 0))
    mandatory = tuple(mandatory_step_count(item) for item in mission.get("workloads", []))
    checkpoints = len(mandatory)
    recovery = max(4, math.ceil(total * 0.10))
    need = sum(mandatory) + checkpoints + 1 + recovery
    if need > total:
        raise BudgetTooSmall(
            f"budget cannot cover mandatory steps (need {need}, have {total})"
        )
    return BudgetPlan(
        total_actions=total,
        mandatory_per_workload=mandatory,
        checkpoint_actions=checkpoints,
        recovery_reserve=recovery,
        probe_allowance=total - need,
    )


class BudgetLedger:
    def __init__(self, plan: BudgetPlan, *, restarts: int) -> None:
        self.plan = plan
        self.actions_spent = 0
        self.restarts_spent = 0
        self.restart_limit = restarts

    @property
    def remaining_actions(self) -> int:
        return max(0, self.plan.total_actions - self.actions_spent)

    def spend(self, kind: str) -> None:
        self.actions_spent += 1
        if kind == "restart":
            self.restarts_spent += 1
            if self.restarts_spent > self.restart_limit:
                raise BudgetTooSmall("restart budget exhausted")

    def may_probe(
        self,
        *,
        remaining_mandatory: int,
        remaining_checkpoints: int,
        safety_margin: int = 3,
    ) -> bool:
        return (
            self.remaining_actions
            - remaining_mandatory
            - remaining_checkpoints
            - self.plan.finish_reserve
            > safety_margin
        )
