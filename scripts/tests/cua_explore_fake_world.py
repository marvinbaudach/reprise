"""A scriptable miniature Reprise observation world for agent acceptance tests."""

from __future__ import annotations

from typing import Any, Mapping

from protocol import ActionGateway
from ui_vocabulary import COLUMN_HEADER_ROLE, RETRY_WORDS, column_header_label
from workload_audit import audit_action_workload


SECTIONS = ("Music", "Queue", "Playlists", "Podcasts", "YouTube", "Radio", "My Stats")
COLUMNS = ("Title", "Artist", "Album", "Year", "Rating")
# The facet popover shows this many values at once and its list is longer.
POPOVER_VISIBLE_ITEMS = 6
POPOVER_SCROLL_ITEMS_PER_PAGE = 4
POPOVER_ITEM_PITCH = 37
POPOVER_TOP = 290
FACET_VALUES = {
    "genre": ("Genre 00 (4,974)", "Genre 01 (4,974)", "Genre 02 (4,974)"),
    "year": tuple(f"{year} (106)" for year in range(1980, 2000)),
    "rating": ("0 (1,658)", "2 (1,658)", "4 (1,658)"),
}
SOURCE_ROWS = {
    "Podcasts": "Fixture Podcast Needle",
    "YouTube": "Fixture YouTube Needle",
    "Radio": "Fixture Radio Needle",
}


class FakeWorld:
    def __init__(
        self,
        *,
        profile: str,
        tokens: Mapping[str, str],
        quirks: frozenset[str] = frozenset(),
    ) -> None:
        self.profile = profile
        self.tokens = dict(tokens)
        self.quirks = quirks
        self.version = 1
        self.section = "Music"
        self.search = ""
        self.connectivity = "online"
        self.offset = 0
        self.scroll_drift = 0
        self.sort_flip = False
        self.selected_count = 0
        self.context_menu = False
        self.context_menu_alternate = False
        self.dialog = False
        self.saving = False
        self.menu_stage: str | None = None
        self.chips: list[str] = []
        self.genre = ""
        self.year = ""
        self.saved = False
        self.popover_offset = 0

    def observation(self) -> dict[str, Any]:
        elements = []
        for index, section in enumerate(SECTIONS):
            if section == "Podcasts" and "no-podcast-section" in self.quirks:
                continue
            if section == "YouTube" and "no-youtube-section" in self.quirks:
                continue
            elements.append(
                self._element(
                    section,
                    "button",
                    y=10 + index * 30,
                    selected=section == self.section,
                )
            )
        if self.section != "My Stats" and not self.dialog:
            value: str | None = self.search
            if "entry-has-no-value" in self.quirks:
                value = None
            elements.append(self._element("Search all fields", "entry", y=10, value=value))
        tree_labels: list[str] = []
        popups: list[dict[str, float]] = []
        if self.dialog:
            # The dialog title is an unindexed label: only the tree holds it.
            tree_labels.extend(["512 of 100,000 tracks", "Edit 512 Tracks"])
            elements.extend(
                [
                    self._element("Genre", "entry", y=120, value=self.genre),
                    self._element("Year", "entry", y=160, value=self.year),
                    self._element("Save 512", "button", y=210),
                ]
            )
        elif self.context_menu:
            if "context-menu-missing" not in self.quirks or self.context_menu_alternate:
                elements.append(self._element("Edit tags…", "menu item", y=300))
        elif self.menu_stage == "facet":
            popups.append(self._popup(3))
            for index, facet in enumerate(("Genre", "Year", "Rating")):
                elements.append(self._element(facet, "menu item", y=300 + index * 30, x=180))
        elif self.menu_stage and self.menu_stage.startswith("value:"):
            popups.append(self._popup(POPOVER_VISIBLE_ITEMS))
            for label, y in self._value_items():
                elements.append(self._element(label, "list item", y=y, x=180))
        else:
            elements.extend(self._surface_elements())
            tree_labels.extend(self._chip_labels())
        if self.selected_count and "no-selection-count" not in self.quirks and not self.dialog:
            elements.append(
                self._element(
                    f"{self.selected_count} selected", "status", y=700, actionable=False
                )
            )
        if self.saving:
            elements.append(self._element("Saving complete", "status", y=740, actionable=False))
        actionable = [item["label"] for item in elements if item["actionable"]]
        return {
            "schema_version": 1,
            "state_id": f"fake-{self.version}",
            "state_signature": f"fake-signature-{self.version}",
            "window": {"width": 1200, "height": 800},
            "degraded": False,
            "actionable_labels": actionable,
            "tree_labels": tree_labels,
            "popups": popups,
            "elements": elements,
        }

    def apply(self, action: Mapping[str, Any]) -> None:
        kind = action.get("kind")
        target = action.get("target", {}).get("label")
        if kind == "activate":
            self._activate(str(target))
        elif kind == "type":
            value = self.tokens.get(str(action.get("fixture_token")), "")
            if target == "Search all fields":
                self.search = value
                if "chip-dropped-by-search" in self.quirks:
                    self.chips.clear()
            elif target == "Genre":
                self.genre = value
            elif target == "Year":
                self.year = value
        elif kind == "press" and action.get("key") == "escape":
            self.search = ""
            self.context_menu = False
        elif kind == "press" and action.get("key") == "f10":
            self.context_menu = True
            self.context_menu_alternate = True
        elif kind == "hotkey":
            keys = action.get("keys")
            if keys == ["ctrl", "a"]:
                self.selected_count = 512
            elif keys == ["shift", "f10"]:
                self.context_menu = "context-menu-missing" not in self.quirks
                self.context_menu_alternate = False
        elif kind == "scroll" and self.menu_stage and self.menu_stage.startswith("value:"):
            step = POPOVER_SCROLL_ITEMS_PER_PAGE * int(action.get("amount", 1))
            last = max(0, len(FACET_VALUES[self.menu_stage.split(":", 1)[1]]) - POPOVER_VISIBLE_ITEMS)
            sign = 1 if action.get("direction") == "down" else -1
            self.popover_offset = min(last, max(0, self.popover_offset + sign * step))
        elif kind == "scroll":
            amount = int(action.get("amount", 1))
            if action.get("direction") == "down":
                self.offset += amount
            else:
                self.offset = max(0, self.offset - amount)
            if "scroll-anchor-drift" in self.quirks:
                self.scroll_drift += 1
        elif kind == "set-connectivity":
            self.connectivity = str(action.get("connectivity"))
        elif kind == "restart":
            self.restart()
        elif kind == "wait":
            if self.saving:
                self.dialog = False
        self.version += 1

    def restart(self) -> None:
        if "search-survives-restart" not in self.quirks:
            self.search = ""
        self.context_menu = False
        self.dialog = False

    def finding_codes(self, action: Mapping[str, Any]) -> list[str]:
        if action.get("kind") == "wait" and action.get("expect_status"):
            return ["missing-waiting-feedback"]
        if action.get("kind") == "hover" and "button-without-hover" in self.quirks:
            return ["hover-affordance-missing"]
        return []

    def _activate(self, label: str) -> None:
        if label in SECTIONS:
            self.section = label
            self.search = ""
            return
        if label.casefold().startswith("refresh"):
            return
        if any(word in label.casefold() for word in RETRY_WORDS):
            return
        if label == "Edit tags…":
            self.context_menu = False
            self.dialog = True
            return
        if label.startswith("Save"):
            self.dialog = True
            self.saving = True
            self.saved = True
            return
        if label == "Add filter":
            self.menu_stage = "facet"
            return
        if self.menu_stage == "facet" and label in {"Genre", "Year", "Rating"}:
            self.menu_stage = f"value:{label.casefold()}"
            self.popover_offset = 0
            return
        if self.menu_stage and self.menu_stage.startswith("value:"):
            facet = self.menu_stage.split(":", 1)[1]
            visible = dict(self._value_items(visible_only=True))
            # A click below the fold lands under the popover: it closes, and no
            # filter is applied.
            self.menu_stage = None
            if label in visible:
                chip = {"genre": "Genre: Genre 00", "year": "Year: 1993", "rating": "Rating: 4"}[facet]
                self.chips.append(chip)
            return
        if label.endswith(column_header_label("")):
            self.sort_flip = not self.sort_flip

    def _surface_elements(self) -> list[dict[str, Any]]:
        elements = []
        if self.section in SOURCE_ROWS:
            elements.append(
                self._element(SOURCE_ROWS[self.section], self._row_role(), y=180)
            )
            if self.connectivity == "offline" and "duplicate-cached-row" in self.quirks:
                elements.append(
                    self._element(SOURCE_ROWS[self.section], self._row_role(), y=220)
                )
            if self.connectivity == "offline" or "offline-status-stuck" in self.quirks:
                elements.append(self._element("You're offline", "label", y=100, actionable=False))
                elements.append(self._element("Try again", "button", y=120))
            else:
                elements.append(self._element("Refresh now", "button", y=120))
            return elements
        if self.section != "Music":
            return elements
        for index, column in enumerate(COLUMNS):
            elements.append(
                self._element(
                    column_header_label(column), COLUMN_HEADER_ROLE, y=80, x=160 + index * 100
                )
            )
        elements.append(self._element("Add filter", "button", y=120))
        rows = self._music_rows()
        for index, label in enumerate(rows):
            elements.append(
                self._element(label, self._row_role(), y=220 + index * 40, x=180)
            )
        return elements

    def _chip_labels(self) -> list[str]:
        """A chip is two unindexed labels: the facet name and its value."""
        labels: list[str] = []
        for chip in self.chips:
            facet, _separator, value = chip.partition(": ")
            labels.extend([facet, value])
        return labels

    def _value_items(self, *, visible_only: bool = False) -> list[tuple[str, float]]:
        facet = (self.menu_stage or "value:genre").split(":", 1)[1]
        items = []
        for index, label in enumerate(FACET_VALUES[facet]):
            y = POPOVER_TOP + (index - self.popover_offset) * POPOVER_ITEM_PITCH
            inside = POPOVER_TOP <= y <= POPOVER_TOP + (POPOVER_VISIBLE_ITEMS - 1) * POPOVER_ITEM_PITCH
            if inside or not visible_only:
                items.append((label, y))
        return items

    def _popup(self, items: int) -> dict[str, float]:
        return {
            "x": 150.0,
            "y": POPOVER_TOP - 10.0,
            "width": 300.0,
            "height": items * POPOVER_ITEM_PITCH + 10.0,
        }

    def _music_rows(self) -> list[str]:
        if self.search:
            if self.search == self.tokens.get("WRITABLE_BATCH"):
                # The whole label is every cell, the year among them; the batch
                # edit rewrites it, the title stays.
                year = 2042 if self.saved else 1980
                rows = [
                    f"Go to album Fixture Album 00 Writable Batch {self.offset * 8 + index + 1:04} "
                    f"Fixture Artist 00 Fixture Album 00 {year} 0:01 \u2014"
                    for index in range(8)
                ]
                return rows[::-1] if self.sort_flip else rows
            expected = self.search
            rows = [expected]
            if "search-leaks-music" in self.quirks:
                rows.append("Leaked Music Result")
            return rows
        base = [f"Track {self.offset * 10 + index:06}" for index in range(8)]
        if self.chips:
            base = [f"Filtered {len(self.chips)} {item}" for item in base]
        if self.sort_flip and "sort-does-not-reorder" not in self.quirks:
            base.reverse()
        return base

    def _row_role(self) -> str:
        if "rows-report-table-row-role" in self.quirks:
            return "unmapped grid record"
        return "row"

    def _element(
        self,
        label: str,
        role: str,
        *,
        x: float = 10,
        y: float = 10,
        actionable: bool = True,
        selected: bool = False,
        value: str | None = None,
    ) -> dict[str, Any]:
        return {
            "key": f"{role}:{label}",
            "label": label,
            "role": role,
            "enabled": True,
            "visible": True,
            "focused": False,
            "selected": selected,
            "value": value,
            "actionable": actionable,
            "frame": {
                "x": x,
                "y": y + self.scroll_drift * 9,
                "width": 100,
                "height": 30,
            },
        }


def drive(session, world: FakeWorld, mission, *, max_actions: int):
    gateway = ActionGateway(mission)
    agent_mission = {
        "schema_version": 1,
        "id": mission.mission_id,
        "budgets": {
            "actions": mission.budgets.actions,
            "seconds": mission.budgets.seconds,
            "restarts": mission.budgets.restarts,
        },
        "capabilities": sorted(mission.capabilities),
        "fixture_tokens": sorted(mission.fixture_tokens),
        "workloads": list(mission.workloads),
        "forbidden": list(mission.forbidden),
    }
    actions = []
    history = []
    observation = world.observation()
    for _index in range(max_actions):
        action = session.next_action(agent_mission, observation, history)
        accepted = gateway.accept(action, observation)
        actions.append(action)
        if action["kind"] == "complete-workload":
            audit = audit_action_workload(
                action["workload_index"],
                mission.workloads[action["workload_index"]],
                session.traces,
                world.tokens,
            )
            if audit.get("complete") is True:
                gateway.confirm_workload(action["workload_index"])
            else:
                history.append({"action": action, "finding_codes": [], "after_state": observation["state_id"]})
                return actions, session.traces
            history.append({"action": action, "finding_codes": [], "after_state": observation["state_id"]})
            continue
        if action["kind"] == "finish":
            break
        world.apply(action)
        finding_codes = world.finding_codes(action)
        observation = world.observation()
        history.append(
            {
                "action": action,
                "finding_codes": finding_codes,
                "after_state": observation["state_id"],
            }
        )
    return actions, session.traces
