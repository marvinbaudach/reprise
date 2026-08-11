#!/usr/bin/env python3
"""Production-path attribution tests for exploratory response gaps."""

from __future__ import annotations

import pathlib
import sys
import time
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "scripts" / "cua-explore"))

from driver import CuaExecutor  # noqa: E402
from oracles import ActionEvidence  # noqa: E402


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
    def __init__(self, final_cpu_ms: int, final_host_load: float) -> None:
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
    def run_gap(self, *, app_cpu_ms: int, host_load_1m: float):
        executor = CuaExecutor(
            DelayedSnapshotTransport(),
            pid=1234,
            window_id=7,
            session="stall-attribution",
            settle_delays=(0.0,),
        )
        executor.activity_probe = ScriptedActivityProbe(app_cpu_ms, host_load_1m)
        return executor.execute_evidence(ActionEvidence.connectivity("online"))

    def test_response_gaps_separate_app_work_from_a_loaded_host(self) -> None:
        busy = self.run_gap(app_cpu_ms=180, host_load_1m=2.0)
        loaded_host = self.run_gap(app_cpu_ms=0, host_load_1m=12.0)

        busy_stall = next(
            finding for finding in busy.findings if finding.code == "main-loop-stall"
        )
        self.assertEqual(busy_stall.evidence["gap_samples"][0]["app_cpu_ms"], 180)
        self.assertEqual(busy_stall.evidence["gap_samples"][0]["host_load_1m"], 2.0)

        loaded_codes = {finding.code for finding in loaded_host.findings}
        self.assertNotIn("main-loop-stall", loaded_codes)
        self.assertIn("environment-load-hint", loaded_codes)
        environment = next(
            finding
            for finding in loaded_host.findings
            if finding.code == "environment-load-hint"
        )
        self.assertEqual(environment.evidence["gap_samples"][0]["app_cpu_ms"], 0)
        self.assertEqual(environment.evidence["gap_samples"][0]["host_load_1m"], 12.0)
        self.assertEqual(
            environment.evidence["gap_samples"][0]["host_load_threshold"], 8.0
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
