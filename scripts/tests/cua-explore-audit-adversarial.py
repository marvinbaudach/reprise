#!/usr/bin/env python3
"""Adversarial regressions for workload-evidence false positives."""

import pathlib
import sqlite3
import sys
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
EXPLORE_ROOT = REPO_ROOT / "scripts" / "cua-explore"
sys.path.insert(0, str(EXPLORE_ROOT))

from protocol import load_mission  # noqa: E402
from fixtures import (  # noqa: E402
    FixtureError,
    _seed_source_rows,
    build_plan,
    validate_scratch_base,
)
from oracles import normalize_snapshot  # noqa: E402
from ui_vocabulary import (  # noqa: E402
    CANONICAL_ROW_ROLE,
    KNOWN_SECTION_LABELS,
    canonical_role,
)
from workload_audit import ActionTrace, audit_action_workload  # noqa: E402


class UiVocabularyContractTests(unittest.TestCase):
    def test_role_aliases_map_table_row_to_the_canonical_row_role(self) -> None:
        self.assertEqual(canonical_role("table row"), CANONICAL_ROW_ROLE)

    def test_normalize_snapshot_canonicalises_row_roles(self) -> None:
        snapshot = normalize_snapshot(
            {
                "structuredContent": {
                    "elements": [{"label": "Track", "role": "table row"}]
                }
            },
            state_id="state-1",
            captured_ms=0,
        )

        self.assertEqual(snapshot.elements[0].role, CANONICAL_ROW_ROLE)

    def test_no_module_redefines_the_busy_role_table(self) -> None:
        definitions = []
        for path in sorted(EXPLORE_ROOT.glob("*.py")):
            for line_number, line in enumerate(
                path.read_text(encoding="utf-8").splitlines(), start=1
            ):
                if line.lstrip().startswith(("BUSY_ROLES =", "busy_roles =")):
                    definitions.append((path.name, line_number))

        self.assertEqual([name for name, _line in definitions], ["ui_vocabulary.py"])

    def test_known_section_labels_contain_podcasts_and_youtube_separately(self) -> None:
        self.assertIn("Podcasts", KNOWN_SECTION_LABELS)
        self.assertIn("YouTube", KNOWN_SECTION_LABELS)
        self.assertNotIn("Podcasts / YouTube", KNOWN_SECTION_LABELS)


class SourceRouteContractTests(unittest.TestCase):
    def test_every_mission_route_label_exists_in_the_known_section_vocabulary(self) -> None:
        labels = set()
        for mission_path in sorted((EXPLORE_ROOT / "missions").glob("*.json")):
            mission = load_mission(mission_path)
            for workload in mission.workloads:
                labels.update(workload.get("route_tokens", {}))
                labels.update(workload.get("source_tokens", {}))
                labels.update(workload.get("unsupported", []))
                labels.update(workload.get("sections", []))

        self.assertLessEqual(labels, set(KNOWN_SECTION_LABELS))

    def test_mixed_sources_profile_seeds_a_youtube_subscription_and_episode(self) -> None:
        connection = sqlite3.connect(":memory:")
        connection.executescript(
            """
            CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE podcast_subscriptions (
                id INTEGER PRIMARY KEY, kind TEXT, feed_url TEXT, title TEXT,
                author TEXT, added_at INTEGER
            );
            CREATE TABLE podcast_episodes (
                id INTEGER PRIMARY KEY, subscription_id INTEGER, guid TEXT,
                title TEXT, audio_url TEXT, published_at INTEGER,
                duration_secs INTEGER, first_seen_at INTEGER
            );
            CREATE TABLE radio_stations (
                id INTEGER PRIMARY KEY, uuid TEXT, name TEXT, stream_url TEXT,
                genre TEXT, added_at INTEGER
            );
            """
        )

        _seed_source_rows(connection)

        self.assertEqual(build_plan("mixed-sources-128").youtube_episode_count, 1)
        self.assertEqual(
            connection.execute(
                """
                SELECT s.kind, s.title, e.title
                FROM podcast_subscriptions AS s
                JOIN podcast_episodes AS e ON e.subscription_id = s.id
                WHERE s.kind = 'youtube'
                """
            ).fetchone(),
            ("youtube", "Fixture Channel", "Fixture YouTube Needle"),
        )
        self.assertEqual(
            connection.execute(
                "SELECT value FROM settings WHERE key = 'module.youtube.enabled'"
            ).fetchone(),
            ("1",),
        )

    def test_section_search_mission_covers_podcasts_and_youtube_separately(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "section-search-isolation.json"
        )
        routes = mission.workloads[0]["route_tokens"]

        self.assertEqual(routes["Podcasts"], "PODCAST_ONLY_NEEDLE")
        self.assertEqual(routes["YouTube"], "YOUTUBE_ONLY_NEEDLE")
        self.assertNotIn("Podcasts / YouTube", routes)


class WorkloadEvidenceAdversarialTests(unittest.TestCase):
    def setUp(self) -> None:
        self.stress = load_mission(
            EXPLORE_ROOT / "missions" / "large-library-stress.json"
        )

    def test_combined_filters_reject_permanent_facet_labels(self) -> None:
        workload = self.stress.workloads[2]
        traces = [
            ActionTrace(
                action={"kind": "activate", "target_label": facet},
                state_changed=True,
            )
            for facet in workload["facets"]
        ]
        traces.append(
            ActionTrace(
                action={
                    "kind": "type",
                    "target_label": "Search all fields",
                    "fixture_token": "SEARCH_NEEDLE",
                },
                after_labels=("Genre", "Year", "Rating"),
                after_rows=(("Needle 099700", 100.0),),
                state_changed=True,
            )
        )

        result = audit_action_workload(
            2, workload, traces, self.stress.fixture_tokens
        )

        self.assertFalse(result["complete"])

    def test_combined_filters_reject_chips_without_changed_result_rows(self) -> None:
        workload = self.stress.workloads[2]
        rows = (("Needle 099700", 100.0),)
        traces = []
        chips = []
        for facet, chip in workload["active_labels"].items():
            chips.append(chip)
            traces.append(
                ActionTrace(
                    action={"kind": "activate", "target_label": facet},
                    before_labels=tuple(chips[:-1]),
                    after_labels=tuple(chips),
                    before_rows=rows,
                    after_rows=(("Needle 099700", 100.0 + len(chips) * 20.0),),
                    state_changed=True,
                )
            )
        traces.append(
            ActionTrace(
                action={
                    "kind": "type",
                    "target_label": "Search all fields",
                    "fixture_token": "SEARCH_NEEDLE",
                },
                after_labels=tuple(chips),
                after_rows=rows,
                state_changed=True,
            )
        )

        result = audit_action_workload(
            2, workload, traces, self.stress.fixture_tokens
        )

        self.assertFalse(result["complete"])

    def test_combined_filters_require_distinct_value_selections(self) -> None:
        workload = self.stress.workloads[2]
        chips = tuple(workload["active_labels"].values())
        traces = [
            ActionTrace(
                action={"kind": "activate", "target_label": "One fake control"},
                after_labels=chips,
                before_rows=(("Before", 100.0),),
                after_rows=(("After", 100.0),),
                state_changed=True,
            ),
            ActionTrace(
                action={
                    "kind": "type",
                    "target_label": "Search all fields",
                    "fixture_token": "SEARCH_NEEDLE",
                },
                after_labels=chips,
                after_rows=(("Needle 099700", 100.0),),
                state_changed=True,
            ),
        ]

        result = audit_action_workload(
            2, workload, traces, self.stress.fixture_tokens
        )

        self.assertFalse(result["complete"])

    def test_section_search_rejects_navigation_and_leaked_rows(self) -> None:
        mission = load_mission(
            EXPLORE_ROOT / "missions" / "section-search-isolation.json"
        )
        workload = {
            "kind": "section-search",
            "route_tokens": {"Radio": "RADIO_ONLY_NEEDLE"},
            "unsupported": ["My Stats"],
        }
        traces = [
            ActionTrace(
                action={"kind": "activate", "target_label": "Radio"},
                after_selected_labels=("Radio",),
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "activate", "target_label": "Music"},
                after_selected_labels=("Music",),
                state_changed=True,
            ),
            ActionTrace(
                action={
                    "kind": "type",
                    "target_label": "Search all fields",
                    "fixture_token": "RADIO_ONLY_NEEDLE",
                },
                before_selected_labels=("Music",),
                after_selected_labels=("Music",),
                after_rows=(
                    ("Fixture Radio Needle", 100.0),
                    ("Foreign leaked result", 140.0),
                ),
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "activate", "target_label": "My Stats"},
                after_actionable_labels=("Music", "Queue"),
                state_changed=True,
            ),
        ]

        result = audit_action_workload(0, workload, traces, mission.fixture_tokens)

        self.assertFalse(result["complete"])

    def test_restart_rejects_search_that_was_never_set_and_then_disappeared(self) -> None:
        workload = {
            "kind": "restart",
            "preserve": ["section"],
            "clear": ["transient-search"],
            "section": "Music",
            "search_token": "SEARCH_NEEDLE",
        }
        trace = ActionTrace(
            action={"kind": "restart"},
            before_selected_labels=("Music", "Selected row"),
            after_selected_labels=("Music", "Different selected row"),
            state_changed=True,
        )

        self.assertFalse(audit_action_workload(0, workload, [trace])["complete"])

    def test_batch_rejects_unbracketed_tokens_and_scroll_anchor(self) -> None:
        workload = self.stress.workloads[0]
        traces = [
            ActionTrace(
                action={
                    "kind": "type",
                    "target_label": "Search all fields",
                    "fixture_token": "BATCH_GENRE",
                },
                state_changed=True,
            ),
            ActionTrace(
                action={
                    "kind": "type",
                    "target_label": "Search all fields",
                    "fixture_token": "BATCH_YEAR",
                },
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "activate", "target_label": "Edit Tags"},
                after_labels=("512 tracks selected",),
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "activate", "target_label": "Apply Changes"},
                after_labels=("512 tracks selected",),
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "wait", "expect_status": True},
                finding_codes=("missing-waiting-feedback",),
            ),
            ActionTrace(
                action={"kind": "scroll", "direction": "down", "by": "page"},
                before_rows=(("Anchor", 100.0),),
                after_rows=(("Anchor", 80.0),),
                state_changed=True,
            ),
            ActionTrace(
                action={"kind": "scroll", "direction": "up", "by": "page"},
                before_rows=(("Anchor", 80.0),),
                after_rows=(("Anchor", 100.0),),
                state_changed=True,
            ),
        ]

        self.assertFalse(audit_action_workload(0, workload, traces)["complete"])

    def test_scroll_rejects_the_oracles_actual_wrong_direction_code(self) -> None:
        workload = {"kind": "scroll-sweep", "directions": ["down"], "pages": 1}
        trace = ActionTrace(
            action={"kind": "scroll", "direction": "down", "by": "page"},
            before_rows=(("Before", 100.0),),
            after_rows=(("After", 100.0),),
            finding_codes=("wrong-scroll-direction",),
            state_changed=True,
        )

        self.assertFalse(audit_action_workload(0, workload, [trace])["complete"])

    def test_offline_restart_rejects_lost_connectivity_status(self) -> None:
        mission = load_mission(EXPLORE_ROOT / "missions" / "offline-recovery.json")
        workload = mission.workloads[1]
        traces = [
            ActionTrace(
                action={"kind": "set-connectivity", "connectivity": "offline"}
            ),
            ActionTrace(
                action={"kind": "restart"},
                before_labels=("No connection · Retry",),
                after_labels=("Online",),
                state_changed=True,
            ),
        ]

        self.assertFalse(
            audit_action_workload(1, workload, traces, mission.fixture_tokens)[
                "complete"
            ]
        )

    def test_scratch_base_validation_rejects_before_creating_any_path(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            protected = pathlib.Path(directory) / "Music"
            with self.assertRaises(FixtureError):
                validate_scratch_base(protected)
            self.assertFalse(protected.exists())


if __name__ == "__main__":
    unittest.main()
