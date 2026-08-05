#!/usr/bin/env python3
"""Fail-closed contracts between exploratory agents and the CUA executor."""

from __future__ import annotations

import argparse
import json
import pathlib
import re
import sys
import time
from dataclasses import dataclass
from typing import Any, Mapping

from actions import (
    AcceptedAction,
    ActivateAction,
    CompleteWorkloadAction,
    ConnectivityAction,
    FinishAction,
    HotkeyAction,
    PressAction,
    ResizeAction,
    RestartAction,
    ScrollAction,
    TypeAction,
    WaitAction,
    action_to_dict,
)


SCHEMA_VERSION = 1
MISSION_ID = re.compile(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
ALLOWED_PROFILES = {
    "empty",
    "mixed-128",
    "mixed-sources-128",
    "writable-512",
    "stress-10k",
    "stress-100k",
}
ALLOWED_MODES = {"discovery", "adversarial", "first-time", "replay"}
ALLOWED_ACTIONS = {
    "activate",
    "type",
    "press",
    "hotkey",
    "scroll",
    "resize",
    "restart",
    "set-connectivity",
    "wait",
    "complete-workload",
    "finish",
}
ALLOWED_ORACLES = {
    "clean-runtime",
    "feedback",
    "waiting-state",
    "layout-shift",
    "pointer-reachability",
    "scroll-direction",
    "main-loop-stall",
    "accessibility",
    "offline-continuity",
}
ALLOWED_KEYS = {
    "tab",
    "enter",
    "escape",
    "space",
    "up",
    "down",
    "left",
    "right",
    "pageup",
    "pagedown",
    "home",
    "end",
    "backspace",
    "delete",
    "f10",
    "a",
    "f",
    "l",
    "m",
    "w",
}
ALLOWED_MODIFIERS = {"ctrl", "shift", "alt"}
ALLOWED_WORKLOADS = {
    "batch-edit",
    "sort-cycle",
    "combined-filter",
    "section-search",
    "scroll-sweep",
    "offline-transition",
    "restart",
}
DESTRUCTIVE_TARGET_WORDS = ("delete", "remove", "forget", "eject", "trash", "erase")
EXTERNAL_TARGET_PHRASES = ("open in browser", "open website", "external link")
MISSION_FIELDS = {
    "schema_version",
    "id",
    "goal",
    "persona",
    "mode",
    "agent",
    "profile",
    "budgets",
    "capabilities",
    "fixture_tokens",
    "oracles",
    "workloads",
    "success",
    "forbidden",
}


class ContractError(ValueError):
    """The agent or mission crossed the explicitly bounded contract."""


def _object(value: Any, name: str) -> Mapping[str, Any]:
    if not isinstance(value, dict):
        raise ContractError(f"{name} must be an object")
    return value


def _string(value: Any, name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ContractError(f"{name} must be a non-empty string")
    return value


def _integer(value: Any, name: str, minimum: int, maximum: int) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ContractError(f"{name} must be an integer")
    if not minimum <= value <= maximum:
        raise ContractError(f"{name} must be between {minimum} and {maximum}")
    return value


def _reject_unknown(value: Mapping[str, Any], allowed: set[str], name: str) -> None:
    unknown = sorted(set(value) - allowed)
    if unknown:
        raise ContractError(f"unknown {name} field: {unknown[0]}")


@dataclass(frozen=True)
class Budgets:
    actions: int
    seconds: int
    restarts: int


@dataclass(frozen=True)
class Mission:
    mission_id: str
    goal: str
    persona: str
    mode: str
    agent: str
    profile: str
    budgets: Budgets
    capabilities: frozenset[str]
    fixture_tokens: Mapping[str, str]
    oracles: tuple[str, ...]
    workloads: tuple[Mapping[str, Any], ...]
    success: tuple[Mapping[str, Any], ...]
    forbidden: tuple[str, ...]


def _parse_budgets(raw: Any) -> Budgets:
    value = _object(raw, "budgets")
    _reject_unknown(value, {"actions", "seconds", "restarts"}, "budget")
    return Budgets(
        actions=_integer(value.get("actions"), "budgets.actions", 1, 500),
        seconds=_integer(value.get("seconds"), "budgets.seconds", 10, 7_200),
        restarts=_integer(value.get("restarts"), "budgets.restarts", 0, 10),
    )


def _parse_string_set(raw: Any, name: str, allowed: set[str]) -> frozenset[str]:
    if not isinstance(raw, list) or not raw:
        raise ContractError(f"{name} must be a non-empty list")
    values = frozenset(_string(item, name) for item in raw)
    unknown = sorted(values - allowed)
    if unknown:
        raise ContractError(f"unknown {name} value: {unknown[0]}")
    return values


def _parse_fixture_tokens(raw: Any) -> Mapping[str, str]:
    value = _object(raw, "fixture_tokens")
    if not value:
        raise ContractError("fixture_tokens must not be empty")
    parsed: dict[str, str] = {}
    for token, fixture_value in value.items():
        if not re.fullmatch(r"[A-Z][A-Z0-9_]*", token):
            raise ContractError(f"invalid fixture token: {token}")
        parsed[token] = _string(fixture_value, f"fixture token {token}")
    return parsed


def _parse_records(raw: Any, name: str) -> tuple[Mapping[str, Any], ...]:
    if not isinstance(raw, list):
        raise ContractError(f"{name} must be a list")
    records = tuple(_object(item, name) for item in raw)
    return records


def _validate_workloads(
    workloads: tuple[Mapping[str, Any], ...], fixture_tokens: Mapping[str, str]
) -> None:
    for workload in workloads:
        kind = _string(workload.get("kind"), "workload.kind")
        if kind not in ALLOWED_WORKLOADS:
            raise ContractError(f"unknown workload kind: {kind}")
        allowed_fields = {
            "batch-edit": {"kind", "selection_count", "field_tokens", "verify"},
            "sort-cycle": {"kind", "columns", "repetitions"},
            "combined-filter": {
                "kind",
                "facets",
                "active_labels",
                "include_search",
                "route",
                "search_token",
            },
            "section-search": {"kind", "route_tokens", "unsupported"},
            "scroll-sweep": {"kind", "directions", "pages"},
            "offline-transition": {"kind", "phases", "interrupt", "source_tokens"},
            "restart": {
                "kind",
                "preserve",
                "clear",
                "connectivity",
                "reason",
                "section",
                "search_token",
                "status_label",
            },
        }[kind]
        _reject_unknown(workload, allowed_fields, f"{kind} workload")
        if kind == "batch-edit":
            _integer(workload.get("selection_count"), "selection_count", 2, 10_000)
            fields = _object(workload.get("field_tokens"), "field_tokens")
            _reject_unknown(fields, {"genre", "year"}, "batch field token")
            if set(fields) != {"genre", "year"}:
                raise ContractError("batch-edit requires genre and year field tokens")
            for token in fields.values():
                if token not in fixture_tokens:
                    raise ContractError(f"unknown batch field token: {token}")
        elif kind == "sort-cycle":
            _integer(workload.get("repetitions"), "repetitions", 1, 200)
        elif kind == "combined-filter":
            facets = workload.get("facets")
            if not isinstance(facets, list) or not facets:
                raise ContractError("combined-filter facets must be a non-empty list")
            active_labels = _object(
                workload.get("active_labels"), "combined-filter active_labels"
            )
            if set(active_labels) != set(facets):
                raise ContractError(
                    "combined-filter active_labels must cover every facet exactly"
                )
            for label in active_labels.values():
                _string(label, "combined-filter active label")
            search_token = workload.get("search_token")
            if search_token is not None and search_token not in fixture_tokens:
                raise ContractError(f"unknown filter search token: {search_token}")
        elif kind == "section-search":
            route_tokens = _object(workload.get("route_tokens"), "route_tokens")
            unsupported = workload.get("unsupported")
            if not route_tokens:
                raise ContractError("section-search route_tokens must not be empty")
            if not isinstance(unsupported, list) or not unsupported:
                raise ContractError("section-search unsupported must be a non-empty list")
            for source, token in route_tokens.items():
                _string(source, "section-search source")
                token_name = _string(token, "section-search token")
                if token_name not in fixture_tokens:
                    raise ContractError(f"unknown section-search token: {token_name}")
        elif kind == "offline-transition":
            source_tokens = _object(
                workload.get("source_tokens"), "offline-transition source_tokens"
            )
            if not source_tokens:
                raise ContractError("offline-transition source_tokens must not be empty")
            for source, token in source_tokens.items():
                _string(source, "offline source")
                token_name = _string(token, "offline source token")
                if token_name not in fixture_tokens:
                    raise ContractError(f"unknown offline source token: {token_name}")
        elif kind == "restart":
            preserve = workload.get("preserve", [])
            clear = workload.get("clear", [])
            if "section" in preserve:
                _string(workload.get("section"), "restart section")
            if "transient-search" in clear:
                search_token = _string(
                    workload.get("search_token"), "restart search_token"
                )
                if search_token not in fixture_tokens:
                    raise ContractError(f"unknown restart search token: {search_token}")
            if workload.get("connectivity") is not None:
                _string(workload.get("status_label"), "restart status_label")


def load_mission(path: pathlib.Path | str) -> Mission:
    mission_path = pathlib.Path(path)
    try:
        raw = json.loads(mission_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ContractError(f"could not read mission: {error}") from error
    value = _object(raw, "mission")
    _reject_unknown(value, MISSION_FIELDS, "mission")
    if value.get("schema_version") != SCHEMA_VERSION:
        raise ContractError(f"mission schema_version must be {SCHEMA_VERSION}")
    mission_id = _string(value.get("id"), "id")
    if not MISSION_ID.fullmatch(mission_id):
        raise ContractError("id must be lowercase kebab-case")
    mode = _string(value.get("mode"), "mode")
    if mode not in ALLOWED_MODES:
        raise ContractError(f"unknown mission mode: {mode}")
    agent = _string(value.get("agent"), "agent")
    if agent not in {"optional", "required"}:
        raise ContractError("agent must be optional or required")
    profile = _string(value.get("profile"), "profile")
    if profile not in ALLOWED_PROFILES:
        raise ContractError(f"unknown fixture profile: {profile}")
    capabilities = _parse_string_set(
        value.get("capabilities"), "capability", ALLOWED_ACTIONS
    )
    oracles = tuple(
        sorted(_parse_string_set(value.get("oracles"), "oracle", ALLOWED_ORACLES))
    )
    fixture_tokens = _parse_fixture_tokens(value.get("fixture_tokens"))
    workloads = _parse_records(value.get("workloads", []), "workloads")
    _validate_workloads(workloads, fixture_tokens)
    success = _parse_records(value.get("success", []), "success")
    forbidden_raw = value.get("forbidden", [])
    if not isinstance(forbidden_raw, list):
        raise ContractError("forbidden must be a list")
    forbidden = tuple(_string(item, "forbidden") for item in forbidden_raw)
    return Mission(
        mission_id=mission_id,
        goal=_string(value.get("goal"), "goal"),
        persona=_string(value.get("persona"), "persona"),
        mode=mode,
        agent=agent,
        profile=profile,
        budgets=_parse_budgets(value.get("budgets")),
        capabilities=capabilities,
        fixture_tokens=fixture_tokens,
        oracles=oracles,
        workloads=workloads,
        success=success,
        forbidden=forbidden,
    )


class ActionGateway:
    """Validates one typed action against the latest observation and budget."""

    def __init__(self, mission: Mission) -> None:
        self.mission = mission
        self._started = time.monotonic()
        self._accepted_actions = 0
        self._accepted_restarts = 0
        self._completed_workloads: set[int] = set()

    def accept(
        self, raw: Mapping[str, Any], observation: Mapping[str, Any]
    ) -> AcceptedAction:
        if self._accepted_actions >= self.mission.budgets.actions:
            raise ContractError("action budget exhausted")
        if time.monotonic() - self._started > self.mission.budgets.seconds:
            raise ContractError("time budget exhausted")
        value = _object(raw, "action")
        if value.get("schema_version") != SCHEMA_VERSION:
            raise ContractError(f"action schema_version must be {SCHEMA_VERSION}")
        state_id = _string(value.get("state_id"), "state_id")
        if state_id != observation.get("state_id"):
            raise ContractError("stale observation state_id")
        kind = _string(value.get("kind"), "kind")
        if kind not in self.mission.capabilities:
            raise ContractError(f"action kind is not allowed: {kind}")
        parser = getattr(self, f"_parse_{kind.replace('-', '_')}", None)
        if parser is None:
            raise ContractError(f"unknown action kind: {kind}")
        action = parser(value, observation)
        if isinstance(action, FinishAction) and self.mission.workloads:
            missing = sorted(set(range(len(self.mission.workloads))) - self._completed_workloads)
            if missing:
                raise ContractError(f"workloads incomplete: {missing}")
        if isinstance(action, RestartAction):
            if self._accepted_restarts >= self.mission.budgets.restarts:
                raise ContractError("restart budget exhausted")
            self._accepted_restarts += 1
        self._accepted_actions += 1
        return action

    def confirm_workload(self, workload_index: int) -> None:
        """Record a checkpoint only after the runner validates retained evidence."""
        if workload_index in self._completed_workloads:
            raise ContractError("workload checkpoint was already recorded")
        self._completed_workloads.add(workload_index)

    def _target(self, value: Mapping[str, Any], observation: Mapping[str, Any]) -> str:
        target = _object(value.get("target"), "target")
        _reject_unknown(target, {"label"}, "target")
        label = _string(target.get("label"), "target.label")
        labels = observation.get("actionable_labels", [])
        if not isinstance(labels, list) or label not in labels:
            raise ContractError(f"target is not actionable in fresh observation: {label}")
        folded = label.casefold()
        if "delete" in self.mission.forbidden and any(
            word in folded for word in DESTRUCTIVE_TARGET_WORDS
        ):
            raise ContractError(f"forbidden target: {label}")
        if "external-url" in self.mission.forbidden and any(
            phrase in folded for phrase in EXTERNAL_TARGET_PHRASES
        ):
            raise ContractError(f"forbidden target: {label}")
        return label

    def _effect_fields(self, value: Mapping[str, Any]) -> tuple[str, str]:
        dispatch = value.get("dispatch", "ax")
        if dispatch not in {"ax", "px"}:
            raise ContractError("dispatch must be ax or px")
        expect_effect = value.get("expect_effect", "required")
        if expect_effect not in {"required", "idempotent", "none"}:
            raise ContractError("expect_effect must be required, idempotent, or none")
        return dispatch, expect_effect

    def _parse_activate(self, value: Mapping[str, Any], observation: Mapping[str, Any]) -> AcceptedAction:
        _reject_unknown(
            value,
            {"schema_version", "state_id", "kind", "target", "dispatch", "expect_effect"},
            "action",
        )
        dispatch, expect_effect = self._effect_fields(value)
        return ActivateAction(
            state_id=value["state_id"],
            target_label=self._target(value, observation),
            dispatch=dispatch,
            expect_effect=expect_effect,
        )

    def _parse_type(self, value: Mapping[str, Any], observation: Mapping[str, Any]) -> AcceptedAction:
        _reject_unknown(
            value,
            {"schema_version", "state_id", "kind", "target", "fixture_token", "dispatch"},
            "action",
        )
        token = _string(value.get("fixture_token"), "fixture_token")
        if token not in self.mission.fixture_tokens:
            raise ContractError(f"unknown fixture token: {token}")
        dispatch, _ = self._effect_fields(value)
        return TypeAction(
            state_id=value["state_id"],
            target_label=self._target(value, observation),
            dispatch=dispatch,
            fixture_token=token,
        )

    def _parse_press(self, value: Mapping[str, Any], observation: Mapping[str, Any]) -> AcceptedAction:
        _reject_unknown(value, {"schema_version", "state_id", "kind", "key", "target"}, "action")
        key = _string(value.get("key"), "key").lower()
        if key not in ALLOWED_KEYS:
            raise ContractError(f"key is not allowed: {key}")
        if key == "delete" and "delete" in self.mission.forbidden:
            raise ContractError("delete key is forbidden by this mission")
        label = self._target(value, observation) if "target" in value else None
        return PressAction(state_id=value["state_id"], target_label=label, key=key)

    def _parse_hotkey(self, value: Mapping[str, Any], observation: Mapping[str, Any]) -> AcceptedAction:
        _reject_unknown(value, {"schema_version", "state_id", "kind", "keys", "target"}, "action")
        keys = value.get("keys")
        if not isinstance(keys, list) or not 2 <= len(keys) <= 3:
            raise ContractError("hotkey.keys must contain two or three keys")
        normalized = tuple(_string(key, "hotkey key").lower() for key in keys)
        if any(key not in ALLOWED_MODIFIERS | ALLOWED_KEYS for key in normalized):
            raise ContractError("hotkey contains an unsupported key")
        if not any(key in ALLOWED_MODIFIERS for key in normalized):
            raise ContractError("hotkey must include a modifier")
        if "delete" in normalized and "delete" in self.mission.forbidden:
            raise ContractError("delete hotkey is forbidden by this mission")
        label = self._target(value, observation) if "target" in value else None
        return HotkeyAction(state_id=value["state_id"], target_label=label, keys=normalized)

    def _parse_scroll(self, value: Mapping[str, Any], observation: Mapping[str, Any]) -> AcceptedAction:
        _reject_unknown(
            value,
            {"schema_version", "state_id", "kind", "direction", "amount", "by", "target"},
            "action",
        )
        direction = value.get("direction")
        by = value.get("by", "page")
        if direction not in {"up", "down", "left", "right"} or by not in {"line", "page"}:
            raise ContractError("scroll direction/by is not allowed")
        amount = _integer(value.get("amount", 1), "scroll.amount", 1, 10)
        label = self._target(value, observation) if "target" in value else None
        return ScrollAction(
            state_id=value["state_id"], target_label=label,
            direction=direction, amount=amount, by=by,
        )

    def _parse_resize(self, value: Mapping[str, Any], _observation: Mapping[str, Any]) -> AcceptedAction:
        _reject_unknown(value, {"schema_version", "state_id", "kind", "width", "height"}, "action")
        return ResizeAction(
            state_id=value["state_id"],
            width=_integer(value.get("width"), "width", 480, 3_840),
            height=_integer(value.get("height"), "height", 360, 2_160),
        )

    def _parse_restart(self, value: Mapping[str, Any], _observation: Mapping[str, Any]) -> AcceptedAction:
        _reject_unknown(value, {"schema_version", "state_id", "kind", "reason"}, "action")
        return RestartAction(state_id=value["state_id"], reason=_string(value.get("reason"), "reason"))

    def _parse_set_connectivity(self, value: Mapping[str, Any], _observation: Mapping[str, Any]) -> AcceptedAction:
        _reject_unknown(value, {"schema_version", "state_id", "kind", "connectivity"}, "action")
        connectivity = value.get("connectivity")
        if connectivity not in {"online", "offline"}:
            raise ContractError("connectivity must be online or offline")
        return ConnectivityAction(state_id=value["state_id"], connectivity=connectivity)

    def _parse_wait(self, value: Mapping[str, Any], _observation: Mapping[str, Any]) -> AcceptedAction:
        _reject_unknown(
            value,
            {"schema_version", "state_id", "kind", "duration_ms", "expect_status"},
            "action",
        )
        expect_status = value.get("expect_status", False)
        if not isinstance(expect_status, bool):
            raise ContractError("expect_status must be a boolean")
        return WaitAction(
            state_id=value["state_id"],
            duration_ms=_integer(value.get("duration_ms"), "duration_ms", 100, 5_000),
            expect_status=expect_status,
        )

    def _parse_complete_workload(
        self, value: Mapping[str, Any], _observation: Mapping[str, Any]
    ) -> AcceptedAction:
        _reject_unknown(
            value,
            {"schema_version", "state_id", "kind", "workload_index"},
            "action",
        )
        index = _integer(
            value.get("workload_index"),
            "workload_index",
            0,
            max(0, len(self.mission.workloads) - 1),
        )
        return CompleteWorkloadAction(
            state_id=value["state_id"],
            workload_index=index,
        )

    def _parse_finish(self, value: Mapping[str, Any], _observation: Mapping[str, Any]) -> AcceptedAction:
        _reject_unknown(value, {"schema_version", "state_id", "kind", "reason"}, "action")
        return FinishAction(state_id=value["state_id"], reason=_string(value.get("reason"), "reason"))


def _mission_summary(mission: Mission) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "id": mission.mission_id,
        "persona": mission.persona,
        "profile": mission.profile,
        "mode": mission.mode,
        "agent": mission.agent,
        "capabilities": sorted(mission.capabilities),
        "oracles": list(mission.oracles),
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["validate-mission", "validate-action"])
    parser.add_argument("mission", type=pathlib.Path)
    args = parser.parse_args(argv)
    try:
        mission = load_mission(args.mission)
        if args.command == "validate-mission":
            json.dump(_mission_summary(mission), sys.stdout, sort_keys=True)
            sys.stdout.write("\n")
            return 0
        raw = json.load(sys.stdin)
        observation = {
            "schema_version": SCHEMA_VERSION,
            "state_id": raw.get("state_id"),
            "actionable_labels": [],
        }
        accepted = ActionGateway(mission).accept(raw, observation)
        json.dump(action_to_dict(accepted), sys.stdout, sort_keys=True)
        sys.stdout.write("\n")
        return 0
    except (ContractError, json.JSONDecodeError) as error:
        print(f"contract rejected: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
