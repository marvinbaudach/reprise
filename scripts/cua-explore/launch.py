#!/usr/bin/env python3
"""Apply and verify the window size declared by an exploratory mission."""

from __future__ import annotations

import json
import os
import pathlib
import re
import subprocess
import time
from typing import Any, Iterator, Mapping, Protocol

from driver import CliTransport, hover_preflight
from fixtures import FixtureError, build_plan
from hover_geometry import WindowGeometry, resolve_window_origin


FAILURE_LOG_PATTERN = re.compile(
    r"Gtk-CRITICAL|GLib-CRITICAL|GLib-GObject-CRITICAL|panicked at|"
    r"BorrowError|BorrowMutError|already borrowed",
    re.IGNORECASE,
)
APP_NAMESPACE_ARGV = ("unshare", "--user", "--map-current-user", "--net", "--")
ACCESSIBILITY_READY_TIMEOUT_SECONDS = 60.0
ACCESSIBILITY_POLL_SECONDS = 0.25
STARTUP_TIMEOUT_BASE_SECONDS = 120.0
STARTUP_TIMEOUT_SECONDS_PER_10K_ROWS = 60.0
STARTUP_TIMEOUT_CAP_SECONDS = 1_200.0
STARTUP_POLL_SECONDS = 0.25




class RunError(RuntimeError):
    """The isolated runner could not establish trustworthy evidence."""


class HoverSmokeComplete(RuntimeError):
    """Internal control flow after a successful preflight-only run."""


def startup_timeout_seconds(profile: str) -> float:
    try:
        track_count = build_plan(profile).track_count
    except FixtureError:
        track_count = 0
    scaled = STARTUP_TIMEOUT_BASE_SECONDS + (
        track_count / 10_000 * STARTUP_TIMEOUT_SECONDS_PER_10K_ROWS
    )
    return min(STARTUP_TIMEOUT_CAP_SECONDS, scaled)


def app_launch_argv(app_binary: pathlib.Path) -> list[str]:
    return [*APP_NAMESPACE_ARGV, str(app_binary)]


def write_gtk_animation_settings(
    profile_root: pathlib.Path, mode: str
) -> pathlib.Path | None:
    if mode == "on":
        return None
    if mode != "off":
        raise RunError("--gtk-animations must be on or off")
    path = profile_root / "config" / "gtk-4.0" / "settings.ini"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("[Settings]\ngtk-enable-animations=0\n", encoding="utf-8")
    return path


def parse_window_origin(value: str | None) -> tuple[int, int] | None:
    if value is None:
        return None
    try:
        x_text, y_text = value.split(",", maxsplit=1)
        return int(x_text), int(y_text)
    except (TypeError, ValueError) as error:
        raise RunError("--window-origin must be X,Y") from error


def measure_cursor_visibility(
    transport: CliTransport,
    *,
    pid: int,
    window_id: int,
    session: str,
    origin: Any,
    evidence_dir: pathlib.Path,
) -> dict[str, Any]:
    from hover_probe import measure_cursor_in_screenshot

    def snapshot(stem: str) -> pathlib.Path:
        path = evidence_dir / f"{stem}.png"
        transport.call(
            "get_window_state",
            {
                "pid": pid,
                "window_id": window_id,
                "session": session,
                "screenshot_out_file": str(path),
            },
        )
        return path

    def move(x: float, y: float) -> None:
        transport.call(
            "move_cursor",
            {
                "pid": pid,
                "window_id": window_id,
                "session": session,
                "scope": "desktop",
                "x": x,
                "y": y,
            },
        )

    return measure_cursor_in_screenshot(snapshot=snapshot, move=move, origin=origin)


def prepare_hover(
    transport: CliTransport,
    *,
    pid: int,
    window_id: int,
    session: str,
    evidence_dir: pathlib.Path,
    window: Mapping[str, Any],
    origin_override: tuple[int, int] | None = None,
) -> tuple[WindowGeometry, dict[str, Any]]:
    if origin_override is None:
        geometry = resolve_window_origin(transport, pid=pid, window_id=window_id)
    else:
        width = window.get("width")
        height = window.get("height")
        if not isinstance(width, int) or not isinstance(height, int):
            raise RunError("hover window dimensions are unavailable")
        geometry = WindowGeometry(*origin_override, width, height)
    cursor = measure_cursor_visibility(
        transport,
        pid=pid,
        window_id=window_id,
        session=session,
        origin=geometry,
        evidence_dir=evidence_dir,
    )
    (evidence_dir / "cursor-visibility.json").write_text(
        json.dumps(cursor, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    evidence = hover_preflight(
        transport,
        pid=pid,
        window_id=window_id,
        session=session,
        origin=geometry,
    )
    (evidence_dir / "hover-preflight.json").write_text(
        json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return geometry, cursor


def private_environment_required() -> None:
    required = {
        "GDK_BACKEND": "x11",
        "WAYLAND_DISPLAY": "",
        "REPRISE_AUDIO_SINK": "fakesink",
    }
    for name, expected in required.items():
        if os.environ.get(name) != expected:
            raise RunError(f"private runner requires {name}={expected!r}")
    for name in ("DISPLAY", "DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR"):
        if not os.environ.get(name):
            raise RunError(f"private runner requires isolated {name}")


def _walk_objects(value: Any) -> Iterator[Mapping[str, Any]]:
    if isinstance(value, dict):
        yield value
        for child in value.values():
            yield from _walk_objects(child)
    elif isinstance(value, list):
        for child in value:
            yield from _walk_objects(child)


def _snapshot_sources(response: Mapping[str, Any]) -> tuple[Mapping[str, Any], ...]:
    structured = response.get("structuredContent")
    return (structured, response) if isinstance(structured, dict) else (response,)


def snapshot_element_count(response: Mapping[str, Any]) -> int:
    for source in _snapshot_sources(response):
        elements = source.get("elements")
        if isinstance(elements, list):
            return len(elements)
    for source in _snapshot_sources(response):
        count = source.get("element_count")
        if isinstance(count, int):
            return count
    return 0


def snapshot_degraded_reason(response: Mapping[str, Any]) -> str:
    for source in _snapshot_sources(response):
        reason = source.get("degraded_reason")
        if isinstance(reason, str) and reason:
            return reason
    return "none reported"


def accessibility_tree_ready(response: Mapping[str, Any]) -> bool:
    degraded = any(
        source.get("degraded") is True for source in _snapshot_sources(response)
    )
    return not degraded and snapshot_element_count(response) > 1


def wait_for_accessibility_tree(
    transport: Any,
    *,
    pid: int,
    window_id: int,
    session: str,
    is_alive: Any | None = None,
    timeout_seconds: float = ACCESSIBILITY_READY_TIMEOUT_SECONDS,
    poll_seconds: float = ACCESSIBILITY_POLL_SECONDS,
    monotonic: Any = time.monotonic,
    sleep: Any = time.sleep,
) -> Mapping[str, Any]:
    deadline = monotonic() + timeout_seconds
    payload = {"pid": pid, "window_id": window_id, "session": session}
    last: Mapping[str, Any] = {}
    while True:
        if is_alive is not None and not is_alive():
            raise RunError("Reprise exited before the accessibility tree appeared")
        last = transport.call("get_window_state", payload)
        if accessibility_tree_ready(last):
            return last
        if monotonic() >= deadline:
            raise RunError(
                "the accessibility tree never became available within "
                f"{timeout_seconds:g}s "
                f"(degraded_reason={snapshot_degraded_reason(last)}, "
                f"element_count={snapshot_element_count(last)})"
            )
        sleep(poll_seconds)


def _window_id(response: Mapping[str, Any]) -> int | None:
    for item in _walk_objects(response):
        candidate = item.get("window_id")
        title = " ".join(
            str(item.get(key, "")) for key in ("title", "class", "wm_class")
        ).casefold()
        if isinstance(candidate, int) and "reprise" in title:
            return candidate
    return None


class AppLifecycle:
    def __init__(
        self,
        *,
        app_binary: pathlib.Path,
        profile_root: pathlib.Path,
        evidence_dir: pathlib.Path,
        connectivity_file: pathlib.Path,
        quit_delay_seconds: int,
        transport: CliTransport,
        session: str = "explore",
        ready_timeout_seconds: float = ACCESSIBILITY_READY_TIMEOUT_SECONDS,
        ready_poll_seconds: float = ACCESSIBILITY_POLL_SECONDS,
        window_timeout_seconds: float = STARTUP_TIMEOUT_BASE_SECONDS,
        window_poll_seconds: float = STARTUP_POLL_SECONDS,
    ) -> None:
        self.app_binary = app_binary
        self.profile_root = profile_root
        self.evidence_dir = evidence_dir
        self.connectivity_file = connectivity_file
        self.quit_delay_seconds = quit_delay_seconds
        self.transport = transport
        self.session = session
        self.ready_timeout_seconds = ready_timeout_seconds
        self.ready_poll_seconds = ready_poll_seconds
        self.window_timeout_seconds = window_timeout_seconds
        self.window_poll_seconds = window_poll_seconds
        self.startup_timings: list[dict[str, int]] = []
        self.process: subprocess.Popen[str] | None = None
        self.log_handle = None
        self.launch_count = 0
        self.log_paths: list[pathlib.Path] = []

    def start(self) -> tuple[int, int, int]:
        if self.process is not None:
            raise RunError("application is already running")
        self.launch_count += 1
        log_path = self.evidence_dir / f"app-{self.launch_count}.log"
        self.log_paths.append(log_path)
        self.log_handle = log_path.open("w", encoding="utf-8")
        environment = {
            **os.environ,
            "XDG_DATA_HOME": str(self.profile_root / "data"),
            "XDG_CACHE_HOME": str(self.profile_root / "cache"),
            "XDG_CONFIG_HOME": str(self.profile_root / "config"),
            "GDK_BACKEND": "x11",
            "WAYLAND_DISPLAY": "",
            "GTK_A11Y": "atspi",
            "NO_AT_BRIDGE": "0",
            "REPRISE_AUDIO_SINK": "fakesink",
            "REPRISE_SMOKE_QUIT": "1",
            "REPRISE_SMOKE_QUIT_DELAY_SECS": str(self.quit_delay_seconds),
            "REPRISE_TEST_CONNECTIVITY_FILE": str(self.connectivity_file),
            "REPRISE_LOG": "debug",
        }
        self.process = subprocess.Popen(
            app_launch_argv(self.app_binary),
            stdout=self.log_handle,
            stderr=subprocess.STDOUT,
            env=environment,
            text=True,
        )
        launched_at = time.monotonic()
        window_id = self._wait_for_window(self.process.pid)
        window_ms = round((time.monotonic() - launched_at) * 1000)
        self._wait_for_accessibility_tree(self.process.pid, window_id)
        self.startup_timings.append(
            {
                "launch": self.launch_count,
                "window_ms": window_ms,
                "accessibility_tree_ms": round(
                    (time.monotonic() - launched_at) * 1000
                ),
            }
        )
        return self.process.pid, window_id, self.launch_count

    def restart(self) -> tuple[int, int, int]:
        self.stop()
        return self.start()

    def stop(self) -> None:
        if self.process is not None:
            process = self.process
            self.process = None
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
        if self.log_handle is not None:
            self.log_handle.close()
            self.log_handle = None

    def assert_clean_logs(self) -> None:
        for path in self.log_paths:
            contents = path.read_text(encoding="utf-8", errors="replace")
            match = FAILURE_LOG_PATTERN.search(contents)
            if match:
                raise RunError(
                    f"application log contains runtime failure marker: {match.group(0)}"
                )
            for required in ("starting Reprise", "database ready"):
                if required not in contents:
                    raise RunError(f"application log is missing '{required}'")

    def _wait_for_window(self, pid: int) -> int:
        started = time.monotonic()
        deadline = started + self.window_timeout_seconds
        while True:
            if self.process is None or self.process.poll() is not None:
                raise RunError("Reprise exited before exposing a window")
            response = self.transport.call("list_windows", {"pid": pid})
            window_id = _window_id(response)
            if window_id is not None:
                return window_id
            if time.monotonic() >= deadline:
                waited = time.monotonic() - started
                raise RunError(
                    "Reprise did not expose a CUA window after "
                    f"{waited:.1f}s (limit {self.window_timeout_seconds:g}s)"
                )
            time.sleep(self.window_poll_seconds)

    def _wait_for_accessibility_tree(
        self, pid: int, window_id: int
    ) -> Mapping[str, Any]:
        return wait_for_accessibility_tree(
            self.transport,
            pid=pid,
            window_id=window_id,
            session=self.session,
            is_alive=lambda: self.process is not None and self.process.poll() is None,
            timeout_seconds=self.ready_timeout_seconds,
            poll_seconds=self.ready_poll_seconds,
        )
