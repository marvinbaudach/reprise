#!/usr/bin/env python3
"""Production-path attribution tests for exploratory response gaps."""

from __future__ import annotations

import os
import pathlib
import sys
import time
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "scripts" / "cua-explore"))

from driver import CuaExecutor  # noqa: E402
from oracles import ActionEvidence  # noqa: E402
from process_activity import ProcessActivityProbe  # noqa: E402


def raw_snapshot() -> dict:
    return {
        "screenshot_width": 800,
        "screenshot_height": 600,
        "elements": [
            {
                "element_index": 0,
                "label": "Reprise",
                "role": "frame",
                "depth": 0,
                "parent_index": None,
                "frame": {"x": 0, "y": 0, "w": 800, "h": 600},
            }
        ],
    }


class DelayedSnapshotTransport:
    def __init__(self) -> None:
        self.delays = iter((0.005, 0.005, 0.35))

    def call(self, tool, payload):
        if tool == "get_window_state":
            time.sleep(next(self.delays))
            return raw_snapshot()
        return {"effect": "confirmed", "verified": True}

    def set_connectivity(self, state):
        return {"effect": "confirmed", "verified": True}


class ScriptedActivityProbe:
    def __init__(self, final_cpu_ms: int | None, final_host_load: float) -> None:
        self.samples = iter(
            (
                {"app_cpu_ms": 0, "host_load_1m": 1.0, "host_cpu_count": 8},
                {"app_cpu_ms": 0, "host_load_1m": 1.0, "host_cpu_count": 8},
                {
                    "app_cpu_ms": final_cpu_ms,
                    "host_load_1m": final_host_load,
                    "host_cpu_count": 8,
                },
            )
        )

    def start(self):
        return None

    def finish(self, started, *, gap_ms):
        return {"gap_ms": gap_ms, **next(self.samples)}


class StallAttributionTests(unittest.TestCase):
    def run_gap(self, *, app_cpu_ms: int | None, host_load_1m: float):
        executor = CuaExecutor(
            DelayedSnapshotTransport(),
            pid=1234,
            window_id=7,
            session="stall-attribution",
            settle_delays=(0.0,),
        )
        executor.activity_probe = ScriptedActivityProbe(app_cpu_ms, host_load_1m)
        return executor.execute_evidence(ActionEvidence.connectivity("online"))

    def test_app_work_remains_a_main_loop_stall(self) -> None:
        busy = self.run_gap(app_cpu_ms=180, host_load_1m=2.0)

        busy_stall = next(
            finding for finding in busy.findings if finding.code == "main-loop-stall"
        )
        self.assertEqual(busy_stall.evidence["attribution"], "app-process-cpu")
        self.assertEqual(busy_stall.evidence["gap_samples"][0]["app_cpu_ms"], 180)
        self.assertEqual(busy_stall.evidence["gap_samples"][0]["host_load_1m"], 2.0)

    def test_loaded_host_remains_an_environment_hint(self) -> None:
        loaded_host = self.run_gap(app_cpu_ms=0, host_load_1m=12.0)

        loaded_codes = {finding.code for finding in loaded_host.findings}
        self.assertNotIn("main-loop-stall", loaded_codes)
        self.assertIn("environment-load-hint", loaded_codes)
        environment = next(
            finding
            for finding in loaded_host.findings
            if finding.code == "environment-load-hint"
        )
        self.assertEqual(environment.evidence["attribution"], "loaded-host")
        self.assertEqual(environment.evidence["gap_samples"][0]["app_cpu_ms"], 0)
        self.assertEqual(environment.evidence["gap_samples"][0]["host_load_1m"], 12.0)
        self.assertEqual(
            environment.evidence["gap_samples"][0]["host_load_threshold"], 8.0
        )

    def test_cpu_idle_gap_on_a_quiet_host_remains_a_product_finding(self) -> None:
        result = self.run_gap(app_cpu_ms=0, host_load_1m=2.0)

        stall = next(
            finding for finding in result.findings if finding.code == "main-loop-stall"
        )
        self.assertEqual(stall.evidence["attribution"], "blocking-round-trip-suspected")
        self.assertIn("blocking round trip", stall.summary)
        self.assertEqual(stall.evidence["gap_samples"][0]["app_cpu_ms"], 0)
        self.assertEqual(stall.evidence["gap_samples"][0]["host_load_1m"], 2.0)
        self.assertEqual(
            stall.evidence["gap_samples"][0]["app_cpu_threshold_ms"], 100
        )
        self.assertEqual(stall.evidence["gap_samples"][0]["host_load_threshold"], 8.0)

    def test_gap_with_unavailable_cpu_measurement_remains_a_product_finding(self) -> None:
        result = self.run_gap(app_cpu_ms=None, host_load_1m=2.0)

        stall = next(
            finding for finding in result.findings if finding.code == "main-loop-stall"
        )
        self.assertEqual(stall.evidence["attribution"], "app-cpu-measurement-unavailable")
        self.assertIn("CPU measurement was unavailable", stall.summary)
        self.assertIsNone(stall.evidence["gap_samples"][0]["app_cpu_ms"])
        self.assertEqual(stall.evidence["gap_samples"][0]["host_load_1m"], 2.0)
        self.assertEqual(
            stall.evidence["gap_samples"][0]["app_cpu_threshold_ms"], 100
        )
        self.assertEqual(stall.evidence["gap_samples"][0]["host_load_threshold"], 8.0)


class ProcessActivityProbeTests(unittest.TestCase):
    def test_real_proc_reader_measures_live_cpu_and_tolerates_a_dead_pid(self) -> None:
        live_probe = ProcessActivityProbe(os.getpid())
        started = live_probe.start()
        burn_until = time.process_time() + 0.08
        while time.process_time() < burn_until:
            pass

        live = live_probe.finish(started, gap_ms=80)

        self.assertIsNotNone(started)
        self.assertGreater(live["app_cpu_ms"], 0)

        pid_max = int(pathlib.Path("/proc/sys/kernel/pid_max").read_text().strip())
        dead_probe = ProcessActivityProbe(pid_max + 1)
        self.assertIsNone(dead_probe.start())
        self.assertIsNone(dead_probe.finish(None, gap_ms=1)["app_cpu_ms"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
