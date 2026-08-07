"""Deterministic, late-bound Reprise exploration agent session."""

from __future__ import annotations

import json
import pathlib
import random
import time
from dataclasses import dataclass
from typing import Any, Callable, Mapping, Sequence

from agents.assertions import assertion_codes, batch_selection_count
from agents.budget import BudgetLedger, BudgetTooSmall, plan_budget
from agents.plans import build_phases
from agents.probes import initial_probe
from agents.sequencer import Sequencer
from agents.steps import step_to_action
from protocol import (
    ALLOWED_KEYS,
    ALLOWED_MODIFIERS,
    DESTRUCTIVE_TARGET_WORDS,
    EXTERNAL_TARGET_PHRASES,
)
from ui_vocabulary import BUSY_ROLES, BUSY_WORDS, is_row
from workload_audit import ActionTrace, audit_action_workload


class AgentGateError(ValueError):
    """The deterministic plan attempted an action the mission cannot accept."""


@dataclass(frozen=True)
class Note:
    code: str
    summary: str
    evidence: Mapping[str, Any]


class TokenLearner:
    def __init__(self) -> None:
        self.values: dict[str, str] = {}

    def observe(
        self,
        observation: Mapping[str, Any],
        last_action: Mapping[str, Any] | None,
    ) -> bool:
        if not last_action or last_action.get("kind") != "type":
            return True
        target = last_action.get("target", {}).get("label")
        token = str(last_action.get("fixture_token", ""))
        for item in observation.get("elements", []):
            if not isinstance(item, dict) or item.get("label") != target:
                continue
            value = item.get("value")
            if isinstance(value, str) and value:
                self.values[token] = value
                return True
        return False


def self_gate(
    action: Mapping[str, Any],
    observation: Mapping[str, Any],
    mission: Mapping[str, Any],
) -> None:
    kind = str(action.get("kind", ""))
    if kind not in mission.get("capabilities", []):
        raise AgentGateError(f"undeclared capability {kind}")
    target = action.get("target")
    if target is not None:
        if not isinstance(target, dict) or set(target) != {"label"}:
            raise AgentGateError("invalid target")
        label = str(target.get("label", ""))
        if label not in observation.get("actionable_labels", []):
            raise AgentGateError("target is not actionable")
        folded = label.casefold()
        if "delete" in mission.get("forbidden", []) and any(
            word in folded for word in DESTRUCTIVE_TARGET_WORDS
        ):
            raise AgentGateError("destructive target")
        if "external-url" in mission.get("forbidden", []) and any(
            phrase in folded for phrase in EXTERNAL_TARGET_PHRASES
        ):
            raise AgentGateError("external target")
    if kind == "type" and action.get("fixture_token") not in mission.get(
        "fixture_tokens", []
    ):
        raise AgentGateError("unknown fixture token")
    if kind == "press" and action.get("key") not in ALLOWED_KEYS:
        raise AgentGateError("unknown key")
    if kind == "hotkey":
        keys = action.get("keys", [])
        if (
            not isinstance(keys, list)
            or not 2 <= len(keys) <= 3
            or keys[0] not in ALLOWED_MODIFIERS
            or any(key not in ALLOWED_KEYS | ALLOWED_MODIFIERS for key in keys)
        ):
            raise AgentGateError("invalid hotkey")
    if kind == "scroll" and not 1 <= int(action.get("amount", 0)) <= 10:
        raise AgentGateError("invalid scroll amount")
    if kind == "wait" and not 100 <= int(action.get("duration_ms", 0)) <= 5_000:
        raise AgentGateError("invalid wait duration")
    if kind == "complete-workload":
        index = int(action.get("workload_index", -1))
        if not 0 <= index < len(mission.get("workloads", [])):
            raise AgentGateError("invalid workload checkpoint")


def observation_to_trace(
    before: Mapping[str, Any],
    after: Mapping[str, Any],
    action: Mapping[str, Any],
    finding_codes: Sequence[str] = (),
) -> ActionTrace:
    def elements(observation: Mapping[str, Any]) -> list[Mapping[str, Any]]:
        return [item for item in observation.get("elements", []) if isinstance(item, dict)]

    def labels(observation: Mapping[str, Any]) -> tuple[str, ...]:
        return tuple(str(item["label"]) for item in elements(observation) if item.get("label"))

    def rows(observation: Mapping[str, Any]) -> tuple[tuple[str, float], ...]:
        result = []
        for item in elements(observation):
            if not item.get("label") or not is_row(str(item.get("role", ""))):
                continue
            frame = item.get("frame", {})
            result.append((str(item["label"]), float(frame.get("y", 0))))
        return tuple(sorted(result, key=lambda item: (item[1], item[0])))

    trace_action = {key: value for key, value in action.items() if key not in {"schema_version", "state_id", "target"}}
    if isinstance(action.get("target"), dict):
        trace_action["target_label"] = action["target"].get("label")
    after_elements = elements(after)
    return ActionTrace(
        action=trace_action,
        before_labels=labels(before),
        after_labels=labels(after),
        before_rows=rows(before),
        after_rows=rows(after),
        before_selected_labels=tuple(
            str(item["label"])
            for item in elements(before)
            if item.get("label") and item.get("selected")
        ),
        after_selected_labels=tuple(
            str(item["label"])
            for item in after_elements
            if item.get("label") and item.get("selected")
        ),
        after_actionable_labels=tuple(str(item) for item in after.get("actionable_labels", [])),
        before_values=tuple(
            (str(item["label"]), str(item.get("value", "")))
            for item in elements(before)
            if item.get("label")
        ),
        after_values=tuple(
            (str(item["label"]), str(item.get("value", "")))
            for item in after_elements
            if item.get("label")
        ),
        after_roles=tuple(
            (str(item["label"]), str(item.get("role", "")))
            for item in after_elements
            if item.get("label")
        ),
        finding_codes=tuple(finding_codes),
        state_changed=before.get("state_signature") != after.get("state_signature"),
        after_busy=any(
            str(item.get("role", "")) in BUSY_ROLES
            or any(word in str(item.get("label", "")).casefold() for word in BUSY_WORDS)
            for item in after_elements
        ),
    )


class AgentSession:
    def __init__(
        self,
        *,
        seed: int,
        notes_dir: pathlib.Path | None = None,
        probe_ratio: float = 1.0,
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        self.seed = seed
        self.notes_dir = notes_dir
        self.probe_ratio = probe_ratio
        self.clock = clock
        self.notes: list[Note] = []
        self.traces: list[ActionTrace] = []
        self.workload_audits: dict[int, Mapping[str, Any]] = {}
        self.learner = TokenLearner()
        self.mission: Mapping[str, Any] | None = None
        self.sequencer: Sequencer | None = None
        self.ledger: BudgetLedger | None = None
        self.started_at: float | None = None
        self.last_observation: Mapping[str, Any] | None = None
        self.last_action: Mapping[str, Any] | None = None
        self.last_step_name: str | None = None
        self.scroll_anchor: dict[str, float] = {}
        self.terminal_reason: str | None = None
        self._probe_done = False
        if self.notes_dir is not None:
            self.notes_dir.mkdir(parents=True, exist_ok=True)
            (self.notes_dir / "agent-notes.jsonl").touch(exist_ok=True)

    def next_action(
        self,
        mission: Mapping[str, Any],
        observation: Mapping[str, Any],
        history: Sequence[Mapping[str, Any]],
    ) -> dict[str, Any]:
        try:
            return self._next_action(mission, observation, history)
        except Exception as error:  # The process must always answer one valid object.
            reason = f"agent-internal-error: {type(error).__name__}: {str(error)[:180]}"
            self.terminal_reason = reason
            return self._finish(observation, reason)

    def _next_action(
        self,
        mission: Mapping[str, Any],
        observation: Mapping[str, Any],
        history: Sequence[Mapping[str, Any]],
    ) -> dict[str, Any]:
        if self.mission is None:
            self._initialize(mission)
        self._record_transition(observation, history)
        if self.terminal_reason is not None:
            return self._finish(observation, self.terminal_reason)
        assert self.mission is not None and self.sequencer is not None and self.ledger is not None
        if self.ledger.remaining_actions <= 1:
            return self._finish(observation, self._finish_reason())
        if not self._probe_done and self.probe_ratio > 0 and self._before_soft_deadline():
            self._probe_done = True
            probe = initial_probe(self.mission, observation, random.Random(self.seed))
            if probe is not None and self.ledger.plan.probe_allowance > 0:
                return self._emit(probe, observation)
        while True:
            phase = self.sequencer.phase
            if phase is None:
                return self._finish(observation, self._finish_reason())
            step = self.sequencer.step
            if step is None:
                audit = audit_action_workload(
                    phase.workload_index,
                    self.mission["workloads"][phase.workload_index],
                    self.traces,
                    self.learner.values,
                )
                self.workload_audits[phase.workload_index] = audit
                if audit.get("complete") is not True:
                    self.add_note(
                        Note(
                            "agent-workload-precheck-incomplete",
                            "The local workload mirror is incomplete; runner audit will decide.",
                            {"workload_index": phase.workload_index, "kind": phase.name},
                        )
                    )
                action = {
                    "schema_version": 1,
                    "state_id": str(observation.get("state_id", "")),
                    "kind": "complete-workload",
                    "workload_index": phase.workload_index,
                }
                self.sequencer.advance_phase()
                return self._emit(action, observation)
            if step.kind == "hover" and "hover" not in self.mission.get("capabilities", []):
                self.sequencer.advance_step()
                continue
            action, role_mismatch = step_to_action(step, observation)
            if role_mismatch:
                self.add_note(
                    Note(
                        f"agent-role-vocabulary-mismatch:{step.name}",
                        "A label matched only after relaxing its expected role.",
                        {"matcher": step.name},
                    )
                )
            if action is not None:
                self.last_step_name = step.name
                self.sequencer.advance_step()
                return self._emit(action, observation)
            if self._can_recover() and self.sequencer.recovery_attempts < 2:
                self.sequencer.recovery_attempts += 1
                return self._emit(
                    {
                        "schema_version": 1,
                        "state_id": str(observation.get("state_id", "")),
                        "kind": "wait",
                        "duration_ms": 500,
                        "expect_status": False,
                    },
                    observation,
                )
            if self.sequencer.alternate_index < len(step.alternates):
                alternate = step.alternates[self.sequencer.alternate_index]
                self.sequencer.alternate_index += 1
                alternate_action, _mismatch = step_to_action(alternate, observation)
                if alternate_action is not None:
                    self.last_step_name = alternate.name
                    return self._emit(alternate_action, observation)
            self.add_note(
                Note(
                    f"agent-missing-affordance:{step.name}",
                    "A required GUI affordance was absent after bounded recovery.",
                    {"actionable_labels": list(observation.get("actionable_labels", []))[:40]},
                )
            )
            self.sequencer.advance_step()

    def _initialize(self, mission: Mapping[str, Any]) -> None:
        if mission.get("schema_version") != 1:
            self.terminal_reason = "agent-contract-mismatch: schema_version must be 1"
            self.mission = mission
            return
        self.mission = mission
        self.started_at = self.clock()
        try:
            plan = plan_budget(mission, self.seed)
            phases = build_phases(mission, self.seed)
        except (BudgetTooSmall, ValueError) as error:
            self.terminal_reason = f"agent-contract-mismatch: {error}"
            return
        capabilities = set(mission.get("capabilities", []))
        missing_capabilities = sorted(
            {
                step.kind
                for phase in phases
                for step in phase.steps
                if step.required and step.kind not in capabilities
            }
        )
        for required in ("complete-workload", "finish"):
            if required not in capabilities:
                missing_capabilities.append(required)
        if missing_capabilities:
            self.terminal_reason = (
                "agent-contract-mismatch: workload requires undeclared capabilities "
                + ", ".join(sorted(set(missing_capabilities)))
            )
            return
        self.ledger = BudgetLedger(plan, restarts=int(mission["budgets"].get("restarts", 0)))
        self.sequencer = Sequencer(phases)

    def _record_transition(
        self,
        observation: Mapping[str, Any],
        history: Sequence[Mapping[str, Any]],
    ) -> None:
        if self.last_observation is not None and self.last_action is not None:
            if self.last_action.get("kind") in {"complete-workload", "finish"}:
                self.last_observation = observation
                self.last_action = None
                self.last_step_name = None
                return
            finding_codes = history[-1].get("finding_codes", []) if history else []
            self.traces.append(
                observation_to_trace(
                    self.last_observation,
                    observation,
                    self.last_action,
                    finding_codes,
                )
            )
            if not self.learner.observe(observation, self.last_action):
                self.add_note(
                    Note(
                        "agent-token-value-unknown",
                        "The typed entry exposed no value in the following observation.",
                        {"fixture_token": str(self.last_action.get("fixture_token", ""))},
                    )
                )
            self._learn_source_token(observation, self.last_action)
            self._evaluate_transition_assertions(
                self.last_observation,
                observation,
                self.last_action,
                self.last_step_name,
            )
            for code, summary, evidence in assertion_codes(
                self.last_action,
                observation,
                self.last_step_name,
                selection_count=batch_selection_count(self.mission or {}),
            ):
                self.add_note(Note(code, summary, evidence))
        self.last_observation = observation
        self.last_action = None
        self.last_step_name = None

    def _emit(
        self, action: dict[str, Any], observation: Mapping[str, Any]
    ) -> dict[str, Any]:
        assert self.mission is not None and self.ledger is not None
        try:
            self_gate(action, observation, self.mission)
        except AgentGateError as error:
            self.add_note(
                Note(
                    f"agent-self-gate-blocked:{str(error).replace(' ', '-')}",
                    "The self-gate replaced an invalid planned action with a safe wait.",
                    {"kind": str(action.get("kind", ""))},
                )
            )
            action = {
                "schema_version": 1,
                "state_id": str(observation.get("state_id", "")),
                "kind": "wait",
                "duration_ms": 250,
                "expect_status": False,
            }
        self.ledger.spend(str(action["kind"]))
        self.last_action = action
        return action

    def _finish(self, observation: Mapping[str, Any], reason: str) -> dict[str, Any]:
        action = {
            "schema_version": 1,
            "state_id": str(observation.get("state_id", "")),
            "kind": "finish",
            "reason": reason[:400],
        }
        if self.ledger is not None and self.ledger.remaining_actions > 0:
            self.ledger.spend("finish")
        self.last_action = action
        return action

    def _finish_reason(self) -> str:
        counts: dict[str, int] = {}
        for note in self.notes:
            counts[note.code] = counts.get(note.code, 0) + 1
        suffix = ", ".join(f"{code}={count}" for code, count in sorted(counts.items()))
        return f"seed={self.seed}; {suffix or 'no agent notes'}"

    def _learn_source_token(
        self,
        observation: Mapping[str, Any],
        action: Mapping[str, Any],
    ) -> None:
        if action.get("kind") != "activate" or self.mission is None:
            return
        source = action.get("target", {}).get("label")
        for workload in self.mission.get("workloads", []):
            token = workload.get("source_tokens", {}).get(source)
            if not token:
                continue
            rows = [
                str(item.get("label"))
                for item in observation.get("elements", [])
                if isinstance(item, dict)
                and item.get("label")
                and is_row(str(item.get("role", "")))
            ]
            if len(rows) == 1:
                self.learner.values[str(token)] = rows[0]

    def _evaluate_transition_assertions(
        self,
        before: Mapping[str, Any],
        after: Mapping[str, Any],
        action: Mapping[str, Any],
        step_name: str | None,
    ) -> None:
        before_labels = [
            str(item.get("label"))
            for item in before.get("elements", [])
            if isinstance(item, dict) and item.get("label")
        ]
        after_labels = [
            str(item.get("label"))
            for item in after.get("elements", [])
            if isinstance(item, dict) and item.get("label")
        ]
        kind = action.get("kind")
        if kind == "type":
            chips = [label for label in before_labels if ": " in label]
            dropped = [label for label in chips if label not in after_labels]
            if dropped:
                self.add_note(
                    Note(
                        "agent-filter-dropped-by-search",
                        "Search removed an active filter chip.",
                        {"labels": dropped[:10]},
                    )
                )
        if kind == "press" and action.get("key") == "escape":
            target = action.get("target", {}).get("label")
            if target == "Search all fields":
                values = {
                    str(item.get("label")): str(item.get("value", ""))
                    for item in after.get("elements", [])
                    if isinstance(item, dict) and item.get("label")
                }
                if values.get("Search all fields", ""):
                    self.add_note(
                        Note(
                            "agent-search-not-cleared",
                            "Escape did not clear the section search.",
                            {"value_length": len(values["Search all fields"])},
                        )
                    )
        if kind == "activate" and action.get("target", {}).get("label") == "My Stats":
            if "Search all fields" in after.get("actionable_labels", []):
                self.add_note(
                    Note(
                        "agent-fake-search-affordance",
                        "A section without search still exposed the global search label.",
                        {"section": "My Stats"},
                    )
                )
        if kind == "restart":
            before_selected = {
                str(item.get("label"))
                for item in before.get("elements", [])
                if isinstance(item, dict) and item.get("selected")
            }
            after_selected = {
                str(item.get("label"))
                for item in after.get("elements", [])
                if isinstance(item, dict) and item.get("selected")
            }
            if before_selected and before_selected != after_selected:
                self.add_note(
                    Note(
                        "agent-section-not-preserved",
                        "Restart changed the selected section.",
                        {"before": sorted(before_selected), "after": sorted(after_selected)},
                    )
                )
        if kind == "set-connectivity" and action.get("connectivity") == "online":
            if any("no connection" in label.casefold() for label in after_labels):
                self.add_note(
                    Note(
                        "agent-offline-status-stuck",
                        "Offline status remained visible after reconnect.",
                        {"labels": [label for label in after_labels if "connection" in label.casefold()]},
                    )
                )
        if kind == "activate" and action.get("target", {}).get("label") in {
            "Podcasts",
            "YouTube",
            "Radio",
        }:
            duplicates = sorted(
                {label for label in after_labels if after_labels.count(label) > 1}
            )
            if duplicates:
                self.add_note(
                    Note(
                        "agent-duplicate-cached-row",
                        "A cached source row appeared more than once.",
                        {"labels": duplicates[:10]},
                    )
                )
        if kind == "activate" and step_name is not None and step_name.startswith("sort-"):
            before_rows = [
                str(item.get("label"))
                for item in before.get("elements", [])
                if isinstance(item, dict) and is_row(str(item.get("role", "")))
            ]
            after_rows = [
                str(item.get("label"))
                for item in after.get("elements", [])
                if isinstance(item, dict) and is_row(str(item.get("role", "")))
            ]
            if before_rows == after_rows:
                self.add_note(
                    Note(
                        "agent-sort-without-reorder",
                        "A sort header activation did not reorder visible rows.",
                        {"row_count": len(after_rows)},
                    )
                )
            if len(before_rows) != len(after_rows):
                self.add_note(
                    Note(
                        "agent-row-count-changed-by-sort",
                        "Sorting changed the number of visible rows.",
                        {"before": len(before_rows), "after": len(after_rows)},
                    )
                )
        if step_name == "anchor-down":
            self.scroll_anchor = {
                str(item.get("label")): float(item.get("frame", {}).get("y", 0))
                for item in before.get("elements", [])
                if isinstance(item, dict) and is_row(str(item.get("role", "")))
            }
        if step_name == "anchor-up-after-edit":
            after_y = {
                str(item.get("label")): float(item.get("frame", {}).get("y", 0))
                for item in after.get("elements", [])
                if isinstance(item, dict) and is_row(str(item.get("role", "")))
            }
            deltas = [
                abs(self.scroll_anchor[label] - after_y[label])
                for label in self.scroll_anchor.keys() & after_y
            ]
            if deltas and min(deltas) > 6:
                self.add_note(
                    Note(
                        "agent-scroll-anchor-lost",
                        "The selected-list scroll anchor was not restored.",
                        {"minimum_delta": min(deltas)},
                    )
                )

    def _before_soft_deadline(self) -> bool:
        assert self.mission is not None and self.started_at is not None
        elapsed = self.clock() - self.started_at
        return elapsed < 0.80 * float(self.mission["budgets"]["seconds"])

    def _can_recover(self) -> bool:
        assert self.mission is not None and self.started_at is not None
        elapsed = self.clock() - self.started_at
        return elapsed < 0.92 * float(self.mission["budgets"]["seconds"])

    def add_note(self, note: Note) -> None:
        self.notes.append(note)
        if self.notes_dir is None:
            return
        self.notes_dir.mkdir(parents=True, exist_ok=True)
        path = self.notes_dir / "agent-notes.jsonl"
        with path.open("a", encoding="utf-8") as handle:
            handle.write(
                json.dumps(
                    {"code": note.code, "summary": note.summary, "evidence": note.evidence},
                    separators=(",", ":"),
                    sort_keys=True,
                )
                + "\n"
            )
