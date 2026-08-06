#!/usr/bin/env python3
"""Model-neutral JSONL adapter for bounded exploratory agents."""

from __future__ import annotations

import json
import os
import selectors
import subprocess
import pathlib
from typing import Any, Mapping, Sequence


MAX_REQUEST_BYTES = 256_000
MAX_RESPONSE_BYTES = 64_000
MAX_HISTORY_ITEMS = 20


class AgentError(RuntimeError):
    """An external agent violated or failed its transport contract."""


class ExternalAgent:
    """Runs an explicitly supplied argv without a shell or credential contract."""

    def __init__(
        self,
        command: Sequence[str],
        timeout_seconds: float = 30.0,
        private_home: pathlib.Path | None = None,
    ) -> None:
        if not command or any(not isinstance(part, str) or not part for part in command):
            raise AgentError("agent command must be a non-empty argv")
        if timeout_seconds <= 0:
            raise AgentError("agent timeout must be positive")
        self.command = tuple(command)
        self.timeout_seconds = timeout_seconds
        self.private_home = private_home
        self._process: subprocess.Popen[str] | None = None

    def __enter__(self) -> "ExternalAgent":
        environment = {
            key: value
            for key, value in os.environ.items()
            if key in {"PATH", "HOME", "LANG", "LC_ALL", "PYTHONPATH"}
        }
        if self.private_home is not None:
            environment["HOME"] = str(self.private_home)
        self._process = subprocess.Popen(
            self.command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
            env=environment,
        )
        return self

    def __exit__(self, _type: object, _value: object, _traceback: object) -> None:
        self.close()

    def propose(
        self,
        mission: Mapping[str, Any],
        observation: Mapping[str, Any],
        history: Sequence[Mapping[str, Any]],
    ) -> Mapping[str, Any]:
        process = self._require_process()
        request = {
            "schema_version": 1,
            "mission": mission,
            "observation": observation,
            "recent_history": list(history[-MAX_HISTORY_ITEMS:]),
            "instruction": (
                "Return exactly one typed JSON action. Never return prose or shell commands. "
                "For enforced workloads, return complete-workload only after performing and "
                "observing that indexed workload."
            ),
        }
        encoded = json.dumps(request, separators=(",", ":"), sort_keys=True)
        if len(encoded.encode("utf-8")) > MAX_REQUEST_BYTES:
            raise AgentError("agent request exceeds the bounded transport size")
        assert process.stdin is not None
        try:
            process.stdin.write(encoded + "\n")
            process.stdin.flush()
        except BrokenPipeError as error:
            raise AgentError(self._exit_message("agent exited before accepting a task")) from error
        line = self._readline()
        if len(line.encode("utf-8")) > MAX_RESPONSE_BYTES:
            raise AgentError("agent response exceeds the bounded transport size")
        try:
            action = json.loads(line)
        except json.JSONDecodeError as error:
            raise AgentError("agent returned invalid JSON") from error
        if not isinstance(action, dict):
            raise AgentError("agent response must be one JSON object")
        return action

    def close(self) -> None:
        if self._process is None:
            return
        process = self._process
        self._process = None
        if process.stdin is not None:
            process.stdin.close()
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=1)
        for stream in (process.stdout, process.stderr):
            if stream is not None:
                stream.close()

    def _require_process(self) -> subprocess.Popen[str]:
        if self._process is None:
            raise AgentError("agent adapter must be used as a context manager")
        if self._process.poll() is not None:
            raise AgentError(self._exit_message("agent process is not running"))
        return self._process

    def _readline(self) -> str:
        process = self._require_process()
        assert process.stdout is not None
        selector = selectors.DefaultSelector()
        try:
            selector.register(process.stdout, selectors.EVENT_READ)
            if not selector.select(self.timeout_seconds):
                raise AgentError("agent response timed out")
            line = process.stdout.readline()
        finally:
            selector.close()
        if not line:
            raise AgentError(self._exit_message("agent closed stdout without an action"))
        return line

    def _exit_message(self, prefix: str) -> str:
        if self._process is None or self._process.stderr is None:
            return prefix
        if self._process.poll() is None:
            return prefix
        stderr = self._process.stderr.read(400).strip()
        return f"{prefix}: {stderr}" if stderr else prefix
