#!/usr/bin/env python3
"""Validated action variants with no impossible internal field combinations."""

from __future__ import annotations

import dataclasses
from dataclasses import dataclass
from typing import Any, ClassVar, Mapping, TypeAlias


@dataclass(frozen=True)
class ActivateAction:
    state_id: str
    target_label: str
    dispatch: str
    expect_effect: str
    kind: ClassVar[str] = "activate"


@dataclass(frozen=True)
class TypeAction:
    state_id: str
    target_label: str
    dispatch: str
    fixture_token: str
    kind: ClassVar[str] = "type"


@dataclass(frozen=True)
class PressAction:
    state_id: str
    key: str
    target_label: str | None = None
    kind: ClassVar[str] = "press"


@dataclass(frozen=True)
class HotkeyAction:
    state_id: str
    keys: tuple[str, ...]
    target_label: str | None = None
    kind: ClassVar[str] = "hotkey"


@dataclass(frozen=True)
class ScrollAction:
    state_id: str
    direction: str
    amount: int
    by: str
    target_label: str | None = None
    kind: ClassVar[str] = "scroll"


@dataclass(frozen=True)
class ResizeAction:
    state_id: str
    width: int
    height: int
    kind: ClassVar[str] = "resize"


@dataclass(frozen=True)
class RestartAction:
    state_id: str
    reason: str
    kind: ClassVar[str] = "restart"


@dataclass(frozen=True)
class ConnectivityAction:
    state_id: str
    connectivity: str
    kind: ClassVar[str] = "set-connectivity"


@dataclass(frozen=True)
class WaitAction:
    state_id: str
    duration_ms: int
    expect_status: bool
    kind: ClassVar[str] = "wait"


@dataclass(frozen=True)
class CompleteWorkloadAction:
    state_id: str
    workload_index: int
    kind: ClassVar[str] = "complete-workload"


@dataclass(frozen=True)
class FinishAction:
    state_id: str
    reason: str
    kind: ClassVar[str] = "finish"


AcceptedAction: TypeAlias = (
    ActivateAction
    | TypeAction
    | PressAction
    | HotkeyAction
    | ScrollAction
    | ResizeAction
    | RestartAction
    | ConnectivityAction
    | WaitAction
    | CompleteWorkloadAction
    | FinishAction
)


def action_to_dict(action: AcceptedAction) -> Mapping[str, Any]:
    """Return the stable wire/report shape, including the class-level discriminator."""
    return {"kind": action.kind, **dataclasses.asdict(action)}
