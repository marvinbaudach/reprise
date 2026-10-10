"""Cross-state assertions the agent evaluates after each transition."""

from __future__ import annotations

import re
from typing import Any, Mapping

from agents.notes import Note
from search_results import result_elements, source_cards
from ui_vocabulary import OFFLINE_STATUS_WORDS, is_row
from workload_audit import SCROLL_ANCHOR_TOLERANCE_PX, anchor_rows, chip_shown


class TransitionAssertions:
    """Mixed into AgentSession; every method reads and writes the session's state."""

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
            # A chip is two unindexed labels now ("Genre", "Genre 00"), so the
            # facets the mission activates are looked up in the tree labels.
            before_tree = tuple(str(label) for label in before.get("tree_labels", []))
            after_tree = tuple(str(label) for label in after.get("tree_labels", []))
            dropped.extend(
                expected
                for expected in self._expected_chips()
                if chip_shown(before_labels, before_tree, expected)
                and not chip_shown(after_labels, after_tree, expected)
            )
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
            if any(
                word in label.casefold()
                for label in after_labels
                for word in OFFLINE_STATUS_WORDS
            ):
                self.add_note(
                    Note(
                        "agent-offline-status-stuck",
                        "Offline status remained visible after reconnect.",
                        {
                            "labels": [
                                label
                                for label in after_labels
                                if any(word in label.casefold() for word in OFFLINE_STATUS_WORDS)
                            ]
                        },
                    )
                )
        if kind == "activate" and action.get("target", {}).get("label") in {
            "Podcasts",
            "YouTube",
            "Radio",
        }:
            # Rows and source cards, not every label: a sidebar entry and its
            # button, or a menu button and its toggle, share one name by design.
            shown = [
                str(item["label"])
                for item in (*result_elements(after), *source_cards(after))
            ]
            duplicates = sorted(
                {label for label in shown if shown.count(label) > 1}
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
            self.scroll_anchor = self._anchor_rows(before)
        if step_name == "anchor-up-after-edit":
            after_y = self._anchor_rows(after)
            lost = [
                abs(self.scroll_anchor[key] - after_y[key])
                for key in self.scroll_anchor.keys() & after_y
                if abs(self.scroll_anchor[key] - after_y[key]) > SCROLL_ANCHOR_TOLERANCE_PX
            ]
            if lost:
                self.add_note(
                    Note(
                        "agent-scroll-anchor-lost",
                        "The selected-list scroll anchor was not restored.",
                        {
                            "rows_moved": len(lost),
                            "rows_shared": len(self.scroll_anchor.keys() & after_y.keys()),
                            "minimum_delta": min(lost),
                        },
                    )
                )

    def _anchor_rows(self, observation: Mapping[str, Any]) -> dict[str, float]:
        """Result rows by title, which is what survives a write of year and genre."""
        pattern = None
        for workload in (self.mission or {}).get("workloads", []):
            if workload.get("kind") == "batch-edit" and workload.get("anchor_title_pattern"):
                pattern = re.compile(str(workload["anchor_title_pattern"]))
        return anchor_rows(
            [
                (str(item["label"]), float(item.get("frame", {}).get("y", 0)))
                for item in result_elements(observation)
            ],
            pattern,
        )

    def _expected_chips(self) -> list[str]:
        return [
            str(label)
            for workload in (self.mission or {}).get("workloads", [])
            if workload.get("kind") == "combined-filter"
            for label in workload.get("active_labels", {}).values()
        ]
