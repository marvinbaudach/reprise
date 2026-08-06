"""Up-front and running action budgets for deterministic exploration."""

from __future__ import annotations

import math
import random
from dataclasses import dataclass
from typing import Any, Mapping

from agents.plans import PLANNERS, build_phases


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


def mandatory_step_count(
    workload: Mapping[str, Any], index: int = 0, *, seed: int = 0
) -> int:
    """Ask the planner how many steps this workload really costs.

    Restating the step count here is how the pre-flight check drifted away from
    the plan it is supposed to guard; plans.py stays the single source.
    """
    kind = workload.get("kind")
    try:
        planner = PLANNERS[str(kind)]
    except KeyError as error:
        raise BudgetTooSmall(f"unknown workload kind: {kind}") from error
    return len(planner(workload, index, random.Random(seed)).steps)


def plan_budget(mission: Mapping[str, Any], seed: int = 0) -> BudgetPlan:
    total = int(mission.get("budgets", {}).get("actions", 0))
    try:
        phases = build_phases(mission, seed)
    except ValueError as error:
        raise BudgetTooSmall(str(error)) from error
    mandatory = tuple(len(phase.steps) for phase in phases)
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
