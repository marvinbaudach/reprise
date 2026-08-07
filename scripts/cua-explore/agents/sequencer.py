"""Phase cursor and bounded missing-affordance recovery state."""

from __future__ import annotations

from dataclasses import dataclass

from agents.steps import Step


@dataclass(frozen=True)
class Phase:
    name: str
    workload_index: int
    steps: tuple[Step, ...]
    order_locked: bool = True


class Sequencer:
    def __init__(self, phases: tuple[Phase, ...]) -> None:
        self.phases = phases
        self.phase_index = 0
        self.step_index = 0
        self.recovery_attempts = 0
        self.alternate_index = 0

    @property
    def phase(self) -> Phase | None:
        if self.phase_index >= len(self.phases):
            return None
        return self.phases[self.phase_index]

    @property
    def step(self) -> Step | None:
        phase = self.phase
        if phase is None or self.step_index >= len(phase.steps):
            return None
        return phase.steps[self.step_index]

    def advance_step(self) -> None:
        self.step_index += 1
        self.recovery_attempts = 0
        self.alternate_index = 0

    def advance_phase(self) -> None:
        self.phase_index += 1
        self.step_index = 0
        self.recovery_attempts = 0
        self.alternate_index = 0
