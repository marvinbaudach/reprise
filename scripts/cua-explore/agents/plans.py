"""Pure mission-to-phase plans for the bundled deterministic agent."""

from __future__ import annotations

import random
import re
from typing import Any, Mapping

from agents.sequencer import Phase
from agents.steps import Step
from agents.vocabulary import BUTTON_MATCHER, LabelMatcher, ROW_MATCHER
from ui_vocabulary import (
    BUTTON_ROLES,
    COLUMN_HEADER_ROLE,
    ENTRY_ROLES,
    RETRY_WORDS,
    SEARCH_ENTRY_LABEL,
    column_header_label,
)

SOURCES_WITHOUT_REFRESH = ("Radio",)
# How often a facet popover is scrolled while its wanted value is below the fold.
POPOVER_SCROLL_STEPS = 6
# The first ROW_MATCHER hit is the sidebar's "Music" row, not a result.
RESULT_ROW_MATCHER = LabelMatcher(roles=ROW_MATCHER.roles, results_only=True)


def _activate(
    name: str,
    label: str,
    *,
    atomic: bool = False,
    token_hint: str | None = None,
) -> Step:
    return Step(
        name,
        "activate",
        LabelMatcher(exact=(label,)),
        {"dispatch": "auto", "expect_effect": "required"},
        atomic_with_next=atomic,
        token_hint=token_hint,
    )


def _sort_by(name: str, column: str) -> Step:
    """Click one column header, which the harness lends to the observation."""
    return Step(
        name,
        "activate",
        LabelMatcher(
            exact=(column_header_label(column),),
            roles=(COLUMN_HEADER_ROLE,),
            strict_roles=True,
        ),
        {"dispatch": "auto", "expect_effect": "required"},
        missing_code=f"agent-column-header-missing:{column}",
    )


def _type(name: str, label: str, token: str) -> Step:
    return Step(
        name,
        "type",
        LabelMatcher(
            exact=(label,), roles=tuple(sorted(ENTRY_ROLES)), strict_roles=True
        ),
        {"fixture_token": token},
        missing_code=f"agent-entry-role-missing:{name}",
    )


def _search_type(name: str, token: str) -> tuple[Step, Step]:
    entry = LabelMatcher(
        exact=(SEARCH_ENTRY_LABEL,),
        roles=tuple(sorted(ENTRY_ROLES)),
        strict_roles=True,
    )
    opener = Step(
        f"open-{name}",
        "activate",
        LabelMatcher(
            exact=(SEARCH_ENTRY_LABEL,),
            roles=tuple(sorted(BUTTON_ROLES)),
            strict_roles=True,
        ),
        {"dispatch": "auto", "expect_effect": "required"},
        required=False,
        skip_when=entry,
    )
    return opener, _type(name, SEARCH_ENTRY_LABEL, token)


def _section_activate(name: str, label: str, **kwargs: Any) -> Step:
    toggle = _activate(f"toggle-sidebar-for-{name}", "Toggle sidebar")
    step = _activate(name, label, **kwargs)
    return Step(
        step.name,
        step.kind,
        step.matcher,
        step.fields,
        alternates=(toggle,),
        required=step.required,
        atomic_with_next=step.atomic_with_next,
        token_hint=step.token_hint,
        missing_code="agent-sidebar-unavailable",
    )


def _hover(name: str) -> Step:
    return Step(name, "hover", BUTTON_MATCHER, required=False)


def plan_section_search(workload: Mapping[str, Any], index: int, rng: random.Random) -> Phase:
    steps = []
    for source, token in workload.get("route_tokens", {}).items():
        steps.extend(
            [
                _section_activate(
                    f"park-before-{source}",
                    "Queue",
                ),
                _section_activate(
                    f"open-{source}",
                    str(source),
                ),
                _hover(f"hover-sample-{source}"),
                *_search_type(f"search-{source}", str(token)),
                Step(
                    f"clear-search-{source}",
                    "press",
                    LabelMatcher(exact=(SEARCH_ENTRY_LABEL,)),
                    {"key": "escape"},
                ),
            ]
        )
    for source in workload.get("unsupported", []):
        steps.append(_section_activate(f"unsupported-{source}", str(source)))
    return Phase("section-search", index, tuple(steps), order_locked=True)


def plan_restart(workload: Mapping[str, Any], index: int, rng: random.Random) -> Phase:
    steps = []
    if workload.get("connectivity") is not None:
        steps.append(
            Step(
                "restart-connectivity",
                "set-connectivity",
                fields={"connectivity": str(workload["connectivity"])},
            )
        )
        steps.append(_section_activate("open-radio-offline", "Radio"))
        steps.append(
            Step(
                "wait-for-offline-status",
                "wait",
                fields={"duration_ms": 500, "expect_status": True},
            )
        )
    else:
        section = str(workload.get("section", "Music"))
        steps.append(_section_activate("restart-section", section))
        token = workload.get("search_token")
        if token:
            steps.extend(_search_type("restart-search", str(token)))
    steps.append(
        Step(
            "restart-app",
            "restart",
            fields={"reason": str(workload.get("reason", "Verify session restoration"))},
        )
    )
    return Phase("restart", index, tuple(steps), order_locked=True)


def _source_visit(
    name: str, source: str, token_hint: str
) -> tuple[Step, Step]:
    """Open a source and, if its episodes are folded away, unfold its card.

    Podcasts and YouTube list a show or a channel as one card and keep its
    episodes behind it; the cached episode is only on screen once the card is
    open. The app keeps a card open across visits, so the card is clicked only
    while the page lists nothing (a second click would fold it again). Radio
    lists its stations directly and needs no click.
    """
    return (
        _section_activate(name, source, token_hint=token_hint),
        Step(
            f"unfold-{name}",
            "activate",
            LabelMatcher(source_cards_only=True),
            {"dispatch": "ax", "expect_effect": "required"},
            required=False,
            skip_when=LabelMatcher(results_strict=True, require_actionable=False),
        ),
    )


def plan_offline_transition(
    workload: Mapping[str, Any], index: int, rng: random.Random
) -> Phase:
    sources = list(workload.get("source_tokens", {}))
    online_sources = list(sources)
    rng.shuffle(online_sources)
    # The refresh action sits in the Podcasts and YouTube footers only; the plan
    # clicks it from the view it ends the online tour on, so Radio goes first.
    online_sources.sort(key=lambda source: source not in SOURCES_WITHOUT_REFRESH)
    token_by_source = workload.get("source_tokens", {})

    def visits(prefix: str, order: list[str]) -> list[Step]:
        return [
            step
            for source in order
            for step in _source_visit(
                f"{prefix}-{source}", source, str(token_by_source.get(source, ""))
            )
        ]

    steps = visits("online", online_sources)
    steps.append(
        Step(
            "refresh-before-offline",
            "activate",
            LabelMatcher(contains=("refresh",)),
            {"dispatch": "ax", "expect_effect": "required"},
            atomic_with_next=True,
        )
    )
    steps.append(
        Step("go-offline", "set-connectivity", fields={"connectivity": "offline"})
    )
    steps.extend(visits("offline", sources))
    steps.append(
        Step(
            "retry-offline",
            "activate",
            LabelMatcher(contains=RETRY_WORDS),
            {"dispatch": "ax", "expect_effect": "required"},
        )
    )
    steps.append(
        Step("go-online", "set-connectivity", fields={"connectivity": "online"})
    )
    steps.extend(visits("recovery", sources))
    return Phase("offline-transition", index, tuple(steps), order_locked=False)


def plan_batch_edit(workload: Mapping[str, Any], index: int, rng: random.Random) -> Phase:
    count = int(workload.get("selection_count", 0))
    fields = workload.get("field_tokens", {})
    context_alternate = Step(
        "context-menu-f10", "press", fields={"key": "f10"}, required=False
    )
    return Phase(
        "batch-edit",
        index,
        (
            *_search_type("find-writable-batch", "WRITABLE_BATCH"),
            # While the popover is open it swallows Ctrl+A and Shift+F10. Ctrl+F
            # closes it and keeps the query (SEARCH-6); Escape would clear it, and
            # a press aimed at the entry plays the focused row after its focus
            # click has dismissed the popover.
            Step("close-search", "hotkey", fields={"keys": ["ctrl", "f"]}),
            # The edit rewrites genre and year. A list sorted by either would
            # reorder by design and the scroll anchor could not tell that from a
            # lost position, so sort by a column the edit leaves alone first.
            *(
                [_sort_by("sort-before-edit", str(workload["sort_by"]))]
                if workload.get("sort_by")
                else []
            ),
            Step("focus-first-row", "activate", RESULT_ROW_MATCHER, {"dispatch": "ax"}),
            Step("anchor-down", "scroll", fields={"direction": "down", "amount": 1, "by": "page"}),
            Step("anchor-up-before-edit", "scroll", fields={"direction": "up", "amount": 1, "by": "page"}),
            Step("position-before-edit", "scroll", fields={"direction": "down", "amount": 1, "by": "page"}),
            Step("select-all", "hotkey", fields={"keys": ["ctrl", "a"]}),
            Step("context-menu", "hotkey", fields={"keys": ["shift", "f10"]}),
            Step(
                "edit-tags",
                "activate",
                LabelMatcher(contains=("edit tags",)),
                # The menu is a popup window; cua-driver has no bounds for its
                # items, so its own click cannot be delivered (0.33.3 and 0.34.0).
                {"dispatch": "px"},
                alternates=(context_alternate,),
            ),
            _type("batch-genre", "Genre", str(fields.get("genre", "BATCH_GENRE"))),
            _type("batch-year", "Year", str(fields.get("year", "BATCH_YEAR"))),
            Step("hover-save", "hover", LabelMatcher(contains=("save", "apply")), required=False),
            Step("save-batch", "activate", LabelMatcher(contains=(f"save {count}", "save", "apply")), {"dispatch": "ax"}),
            Step("wait-for-write-1", "wait", fields={"duration_ms": 2_000, "expect_status": True}),
            Step("wait-for-write-2", "wait", fields={"duration_ms": 5_000, "expect_status": True}),
            Step("wait-for-write-3", "wait", fields={"duration_ms": 5_000, "expect_status": True}),
            Step("wait-for-write-4", "wait", fields={"duration_ms": 5_000, "expect_status": True}),
            Step("wait-for-write-5", "wait", fields={"duration_ms": 5_000, "expect_status": True}),
            Step("wait-for-write-6", "wait", fields={"duration_ms": 5_000, "expect_status": True}),
            Step("anchor-up-after-edit", "scroll", fields={"direction": "up", "amount": 1, "by": "page"}),
        ),
        order_locked=True,
    )


def plan_sort_cycle(workload: Mapping[str, Any], index: int, rng: random.Random) -> Phase:
    columns = [str(item) for item in workload.get("columns", [])]
    start = rng.randrange(len(columns)) if columns else 0
    direction = rng.choice((-1, 1))
    ordered = (
        [
            columns[(start + direction * offset) % len(columns)]
            for offset in range(len(columns))
        ]
        if columns
        else []
    )
    repetitions = int(workload.get("repetitions", 0))
    cycle = (
        (ordered * ((repetitions + len(ordered) - 1) // len(ordered)))[:repetitions]
        if ordered
        else []
    )
    steps = [
        Step(
            "clear-search-before-sort",
            "press",
            LabelMatcher(exact=(SEARCH_ENTRY_LABEL,)),
            {"key": "escape"},
        ),
        *[
            _sort_by(f"sort-{number}-{column}", column)
            for number, column in enumerate(cycle)
        ],
    ]
    return Phase("sort-cycle", index, tuple(steps), order_locked=False)


def plan_combined_filter(
    workload: Mapping[str, Any], index: int, rng: random.Random
) -> Phase:
    steps = [
        Step(
            "clear-search-before-filters",
            "press",
            LabelMatcher(exact=(SEARCH_ENTRY_LABEL,)),
            {"key": "escape"},
        )
    ]
    active = workload.get("active_labels", {})
    for facet in workload.get("facets", []):
        value = str(active.get(facet, ""))
        option = value.split(":", maxsplit=1)[-1].strip()
        # A value is listed with its count ("1993 (106)"). The popover list is
        # taller than the popover, so a value below the fold is first scrolled
        # into it; a click aimed at the position the tree reports for it lands
        # on whatever lies under the popover instead.
        listed_value = LabelMatcher(
            patterns=(rf"{re.escape(option)}(?: \(\d[\d,]*\))?",), in_popup=True
        )
        any_listed_value = LabelMatcher(patterns=(r".+ \(\d[\d,]*\)",), in_popup=True)
        steps.extend(
            [
                _activate(f"add-filter-{facet}", "Add filter"),
                Step(
                    f"choose-facet-{facet}",
                    "activate",
                    LabelMatcher(exact=(str(facet).title(),), contains=(str(facet),)),
                    {"dispatch": "ax"},
                ),
                *[
                    Step(
                        f"scroll-{facet}-values-{number}",
                        "scroll",
                        any_listed_value,
                        {"direction": "down", "amount": 1, "by": "page"},
                        required=False,
                        skip_when=listed_value,
                    )
                    for number in range(POPOVER_SCROLL_STEPS)
                ],
                Step(
                    f"choose-value-{facet}",
                    "activate",
                    listed_value,
                    {"dispatch": "ax"},
                ),
            ]
        )
    if workload.get("include_search"):
        steps.append(
            _type(
                "combined-filter-search",
                SEARCH_ENTRY_LABEL,
                str(workload.get("search_token")),
            )
        )
    return Phase("combined-filter", index, tuple(steps), order_locked=True)


def plan_scroll_sweep(workload: Mapping[str, Any], index: int, rng: random.Random) -> Phase:
    steps = [
        Step(
            "clear-search-before-scroll",
            "press",
            LabelMatcher(exact=(SEARCH_ENTRY_LABEL,)),
            {"key": "escape"},
        ),
        # A facet left active by the filter workload shrinks the list, and the
        # sweep only counts pages that moved the rows.
        Step(
            "clear-filters-before-scroll",
            "activate",
            LabelMatcher(contains=("clear all",)),
            {"dispatch": "ax"},
            required=False,
        ),
    ]
    pages = int(workload.get("pages", 0))
    for direction in workload.get("directions", []):
        remaining = pages
        while remaining:
            maximum = min(10, remaining)
            amount = maximum if remaining <= 10 else rng.randint(max(5, maximum - 3), maximum)
            steps.append(
                Step(
                    f"scroll-{direction}-{len(steps)}",
                    "scroll",
                    fields={"direction": str(direction), "amount": amount, "by": "page"},
                )
            )
            remaining -= amount
    return Phase("scroll-sweep", index, tuple(steps), order_locked=False)


PLANNERS = {
    "section-search": plan_section_search,
    "restart": plan_restart,
    "offline-transition": plan_offline_transition,
    "batch-edit": plan_batch_edit,
    "sort-cycle": plan_sort_cycle,
    "combined-filter": plan_combined_filter,
    "scroll-sweep": plan_scroll_sweep,
}


def build_phases(mission: Mapping[str, Any], seed: int) -> tuple[Phase, ...]:
    rng = random.Random(seed)
    phases = []
    for index, workload in enumerate(mission.get("workloads", [])):
        kind = str(workload.get("kind", ""))
        try:
            planner = PLANNERS[kind]
        except KeyError as error:
            raise ValueError(f"unknown workload kind: {kind}") from error
        phases.append(planner(workload, index, rng))
    return tuple(phases)
