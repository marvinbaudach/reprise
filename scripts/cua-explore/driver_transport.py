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
        command = [
            self.driver_binary,
            tool,
            json.dumps(payload, separators=(",", ":")),
        ]
        if self.socket_path is not None:
            command.extend(["--socket", str(self.socket_path)])
        attempt = 0
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
                if completed.stdout.strip().startswith("✅"):
                    return {"effect": "confirmed", "verified": True}
                self._retain_fault(tool, attempt, completed)
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
                raise DriverError(response_error)
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


def _response_error(tool: str, response: Mapping[str, Any]) -> str | None:
    refusal = response.get("refusal")
    if refusal is not None:
        details = refusal if isinstance(refusal, Mapping) else {}
        code = str(details.get("code") or "refused")
        message = str(details.get("message") or refusal)
        return f"cua-driver {tool} refused [{code}]: {message}"
    status = response.get("status")
    if status is None:
        return None
    if isinstance(status, str) and status.casefold() in SUCCESS_STATUSES:
        return None
    return f"cua-driver {tool} returned unsuccessful status: {status!r}"


def response_dispatched(response: Mapping[str, Any]) -> bool:
    """Return whether an accepted action response proves dispatch occurred."""

    if _response_error("action", response) is not None:
        return False
    status = response.get("status")
    if isinstance(status, str) and status.casefold() in SUCCESS_STATUSES:
        return True
    return any(
        key in response for key in ("effect", "route", "delivery", "verified")
    )
