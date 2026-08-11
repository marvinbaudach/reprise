#!/usr/bin/env python3
"""No-shell transport adapter and response contract for cua-driver."""

from __future__ import annotations

import json
import os
import pathlib
import subprocess
import time
from typing import Any, Mapping, Protocol, Sequence

from driver_faults import append_fault
from oracles import Finding


class DriverError(RuntimeError):
    """The native driver or target window could not honor the bounded action."""


RETRYABLE_TOOLS = frozenset(
    {"get_window_state", "get_cursor_position", "get_screen_size"}
)
RETRY_DELAYS_SECONDS = (0.25, 0.50)
SUCCESS_STATUSES = frozenset({"ok", "success", "succeeded"})
BACKGROUND_UNAVAILABLE_CODE = "background_unavailable"
# Measured from the cua-driver 0.19.3 schemas used by the mission actions.
# Escalation is still response-driven: these tools stay on their default
# background route unless the driver returns BACKGROUND_UNAVAILABLE_CODE.
DELIVERY_MODE_TOOLS = frozenset({"type_text", "press_key", "hotkey"})
# The driver reports a failed delivery next to a normal-looking outcome instead
# of failing the call. Measured on cua-driver 0.19.3: a type_text that fell
# through returns {"delivery":{"mode":"background"},"effect":"unverifiable",
# "escalation":{"reason":"delivery_failed","target":"foreground"},...}.
DELIVERY_FAILED_REASON = "delivery_failed"
# A driver line that reads as a confirmation to a human but is not JSON.
HUMAN_CONFIRMATION_PREFIX = "✅"

# What a successful response has to carry, per tool. cua-driver has no single
# success marker and at least three error shells - {"status":"refused",
# "refusal":{...}}, a bare {"code":...} object, and a non-zero exit - so
# enumerating the failures is how a fourth shell gets read as a success. A
# response has to prove instead that it is the answer the tool promises.
# Measured against cua-driver 0.19.3 with `describe <tool>` plus one private
# Xvfb session per tool (2026-08-10); the errors that session produced -
# {"code":"background_unavailable",...} for press_key/hotkey and
# {"code":"desktop_escalation_required",...} for get_cursor_position - carry
# neither `status` nor `refusal` and are caught by the missing payload alone.
ACTION_OUTCOME_KEYS = frozenset({"effect"})
SUCCESS_CONTRACT: Mapping[str, frozenset[str]] = {
    "click": ACTION_OUTCOME_KEYS,
    "type_text": ACTION_OUTCOME_KEYS,
    "press_key": ACTION_OUTCOME_KEYS,
    "hotkey": ACTION_OUTCOME_KEYS,
    "scroll": ACTION_OUTCOME_KEYS,
    "move_cursor": ACTION_OUTCOME_KEYS,
    "get_window_state": frozenset({"elements"}),
    "get_cursor_position": frozenset({"x", "y"}),
    "get_screen_size": frozenset({"width", "height"}),
    "list_windows": frozenset({"windows"}),
    "set_agent_cursor_enabled": frozenset({"enabled"}),
}


class Transport(Protocol):
    def call(self, tool: str, payload: Mapping[str, Any]) -> Mapping[str, Any]: ...
    def resize_window(
        self, window_id: int, width: int, height: int
    ) -> Mapping[str, Any]: ...
    def set_connectivity(self, state: str) -> Mapping[str, Any]: ...
    def wmctrl_geometry(self, window_id: int) -> Any: ...


class CliTransport:
    """No-shell adapter for cua-driver, wmctrl, and test connectivity."""

    def __init__(
        self,
        *,
        driver_binary: str = "cua-driver",
        socket_path: pathlib.Path | None = None,
        connectivity_file: pathlib.Path | None = None,
        evidence_dir: pathlib.Path | None = None,
        timeout_seconds: int = 30,
    ) -> None:
        self.driver_binary = driver_binary
        self.socket_path = socket_path
        self.connectivity_file = connectivity_file
        self.evidence_dir = evidence_dir
        self.timeout_seconds = timeout_seconds
        self.transport_faults = 0
        self._fault_finding_emitted = False
        self._retained_fault_lines = 0

    def call(self, tool: str, payload: Mapping[str, Any]) -> Mapping[str, Any]:
        request_payload = dict(payload)
        command = [
            self.driver_binary,
            tool,
            json.dumps(request_payload, separators=(",", ":")),
        ]
        if self.socket_path is not None:
            command.extend(["--socket", str(self.socket_path)])
        attempt = 0
        delivery_escalation: dict[str, str] | None = None
        while True:
            attempt += 1
            try:
                completed = self._run(command)
            except subprocess.TimeoutExpired as error:
                self._retain_fault(tool, attempt, error)
                if tool in RETRYABLE_TOOLS and attempt <= len(RETRY_DELAYS_SECONDS):
                    time.sleep(RETRY_DELAYS_SECONDS[attempt - 1])
                    continue
                raise DriverError(f"cua-driver {tool} timed out") from error
            if completed.returncode != 0:
                self._retain_fault(tool, attempt, completed)
                message = completed.stderr.strip() or completed.stdout.strip()
                raise DriverError(f"cua-driver {tool} failed: {message[:500]}")
            try:
                response = json.loads(completed.stdout)
            except json.JSONDecodeError as error:
                self._retain_fault(tool, attempt, completed)
                confirmation = _human_confirmation(tool, completed.stdout)
                if confirmation is not None:
                    return confirmation
                if tool in RETRYABLE_TOOLS and attempt <= len(RETRY_DELAYS_SECONDS):
                    time.sleep(RETRY_DELAYS_SECONDS[attempt - 1])
                    continue
                raise DriverError(
                    f"cua-driver {tool} returned invalid JSON"
                ) from error
            if not isinstance(response, dict):
                raise DriverError(
                    f"cua-driver {tool} returned a non-object response"
                )
            response_error = _response_error(tool, response)
            if response_error is not None:
                self._retain_fault(tool, attempt, completed, response=response)
                if (
                    delivery_escalation is None
                    and _can_retry_in_foreground(tool, request_payload, response)
                ):
                    delivery_escalation = {
                        "code": BACKGROUND_UNAVAILABLE_CODE,
                        "from": "background",
                        "to": "foreground",
                    }
                    request_payload["delivery_mode"] = "foreground"
                    command[2] = json.dumps(
                        request_payload, separators=(",", ":")
                    )
                    continue
                raise DriverError(response_error)
            delivery_failure = _delivery_failure(response)
            if delivery_failure is not None:
                # The tool answered its contract and reported in the same
                # breath that the input never arrived. That is evidence, not a
                # reason to end the ordinary background route: the caller
                # reads it through response_dispatched and draws no product
                # verdict from it. After the one foreground escape, however,
                # there is no further safe delivery route to try.
                self._retain_fault(tool, attempt, completed, response=response)
                if delivery_escalation is not None:
                    raise DriverError(
                        f"cua-driver {tool} foreground delivery failed: "
                        f"{delivery_failure}"
                    )
            if delivery_escalation is not None:
                return {**response, "delivery_escalation": delivery_escalation}
            return response

    def _run(self, command: Sequence[str]) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            timeout=self.timeout_seconds,
            env={**os.environ, "CUA_DRIVER_RS_UPDATE_CHECK": "0"},
        )

    def _retain_fault(
        self,
        tool: str,
        attempt: int,
        result: Any,
        *,
        response: Mapping[str, Any] | None = None,
    ) -> None:
        self.transport_faults += 1
        if self.evidence_dir is None:
            return
        self._retained_fault_lines = append_fault(
            self.evidence_dir,
            self._retained_fault_lines,
            tool=tool,
            attempt=attempt,
            result=result,
            response=response,
        )

    def take_findings(self) -> list[Finding]:
        if not self.transport_faults or self._fault_finding_emitted:
            return []
        self._fault_finding_emitted = True
        return [
            Finding(
                "driver-transport-fault",
                "warning",
                0.9,
                "A driver call failed and its payload was retained.",
                {"transport_faults": self.transport_faults},
                blocks_gate=False,
            )
        ]

    def resize_window(
        self, window_id: int, width: int, height: int
    ) -> Mapping[str, Any]:
        completed = subprocess.run(
            [
                "wmctrl",
                "-i",
                "-r",
                f"0x{window_id:x}",
                "-e",
                f"0,-1,-1,{width},{height}",
            ],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
        if completed.returncode != 0:
            raise DriverError(
                f"wmctrl resize failed: {completed.stderr.strip()[:300]}"
            )
        return {"effect": "unverifiable", "verified": False}

    def set_connectivity(self, state: str) -> Mapping[str, Any]:
        if self.connectivity_file is None:
            raise DriverError("connectivity perturbation has no private control file")
        self.connectivity_file.write_text(state + "\n", encoding="utf-8")
        return {"effect": "confirmed", "verified": True}

    def wmctrl_geometry(self, window_id: int) -> Any:
        from hover_geometry import WindowGeometry

        completed = subprocess.run(
            ["wmctrl", "-lG"],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
        if completed.returncode != 0:
            raise DriverError(
                f"wmctrl geometry failed: {completed.stderr.strip()[:300]}"
            )
        expected = f"0x{window_id:08x}".casefold()
        for line in completed.stdout.splitlines():
            fields = line.split(maxsplit=7)
            if len(fields) < 6 or fields[0].casefold() != expected:
                continue
            try:
                x, y, width, height = (int(value) for value in fields[2:6])
            except ValueError as error:
                raise DriverError("wmctrl returned invalid window geometry") from error
            return WindowGeometry(x, y, width, height)
        raise DriverError("wmctrl did not find the target window")


def _human_confirmation(tool: str, stdout: str) -> Mapping[str, Any] | None:
    """Return what a non-JSON confirmation line is worth for this tool.

    The line is written for a human, so it proves nothing beyond "something
    happened": it can stand in for an action outcome, never for a tool that
    owes us data. Every occurrence is retained by the caller as a fault.
    """

    if not stdout.strip().startswith(HUMAN_CONFIRMATION_PREFIX):
        return None
    confirmation = {"effect": "unverifiable", "verified": False}
    if _response_error(tool, confirmation) is not None:
        return None
    return confirmation


def _can_retry_in_foreground(
    tool: str, payload: Mapping[str, Any], response: Mapping[str, Any]
) -> bool:
    """Allow the one documented escape only where the tool schema accepts it."""

    return (
        tool in DELIVERY_MODE_TOOLS
        and payload.get("delivery_mode") != "foreground"
        and response.get("code") == BACKGROUND_UNAVAILABLE_CODE
    )


def _payload(response: Mapping[str, Any]) -> Mapping[str, Any]:
    """Return the object that carries the tool's answer.

    get_window_state documents both a top-level `elements` array and the same
    data under `structuredContent`; every other tool answers flat.
    """

    structured = response.get("structuredContent")
    return structured if isinstance(structured, Mapping) else response


def _rejection_error(tool: str, response: Mapping[str, Any]) -> str | None:
    """Return the error in a response that names its own failure."""

    refusal = response.get("refusal")
    if refusal is not None:
        details = refusal if isinstance(refusal, Mapping) else {}
        code = str(details.get("code") or "refused")
        message = str(details.get("message") or refusal)
        return f"cua-driver {tool} refused [{code}]: {message}"
    status = response.get("status")
    if status is not None and not (
        isinstance(status, str) and status.casefold() in SUCCESS_STATUSES
    ):
        return f"cua-driver {tool} returned unsuccessful status: {status!r}"
    return None


def _delivery_failure(response: Mapping[str, Any]) -> str | None:
    """Return the escalation a response asks for after a failed delivery.

    This shell answers the tool's contract - it carries `effect` like any other
    outcome - and says next to it that the input never arrived. It is the
    reason the contract cannot be a list of known failures.
    """

    escalation = response.get("escalation")
    if not isinstance(escalation, Mapping):
        return None
    if str(escalation.get("reason") or "") != DELIVERY_FAILED_REASON:
        return None
    return str(escalation.get("target") or "unknown")


def _response_error(tool: str, response: Mapping[str, Any]) -> str | None:
    """Return why a response is not the success its tool promises, or None.

    Enumerating the known error shells is what let the third one through, so
    the rule is the other way round: a response counts as a success only when
    it carries the payload SUCCESS_CONTRACT names for that tool.
    """

    rejection = _rejection_error(tool, response)
    if rejection is not None:
        return rejection
    required = SUCCESS_CONTRACT.get(tool)
    if required is None:
        return (
            f"cua-driver {tool} has no success contract; add one to "
            "SUCCESS_CONTRACT before the harness calls it"
        )
    payload = _payload(response)
    missing = sorted(key for key in required if key not in payload)
    if missing:
        return (
            f"cua-driver {tool} answered without {', '.join(missing)}: "
            f"{json.dumps(response, sort_keys=True, default=str)[:300]}"
        )
    return None


def response_dispatched(response: Mapping[str, Any]) -> bool:
    """Return whether an accepted action response proves dispatch occurred."""

    if _rejection_error("action", response) is not None:
        return False
    if _delivery_failure(response) is not None:
        return False
    status = response.get("status")
    if isinstance(status, str) and status.casefold() in SUCCESS_STATUSES:
        return True
    return any(
        key in response for key in ("effect", "route", "delivery", "verified")
    )
