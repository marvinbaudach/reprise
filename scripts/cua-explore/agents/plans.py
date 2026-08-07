"""Pure mission-to-phase plans for the bundled deterministic agent."""

from __future__ import annotations

import random
from typing import Any, Mapping

from agents.sequencer import Phase
from agents.steps import Step
from agents.vocabulary import BUTTON_MATCHER, LabelMatcher, ROW_MATCHER
from ui_vocabulary import ENTRY_ROLES, SEARCH_ENTRY_LABEL


def _activate(
    name: str,
    label: str,
    *,
    atomic: bool = False,
    token_hint: str | None = None,
    dispatch: str = "ax",
) -> Step:
    return Step(
        name,
        "activate",
        LabelMatcher(exact=(label,)),
        {"dispatch": dispatch, "expect_effect": "required"},
        atomic_with_next=atomic,
        token_hint=token_hint,
    )


def _type(name: str, label: str, token: str) -> Step:
    return Step(
        name,
        "type",
        LabelMatcher(exact=(label,), roles=tuple(sorted(ENTRY_ROLES))),
        {"fixture_token": token},
    )


def _hover(name: str) -> Step:
    return Step(name, "hover", BUTTON_MATCHER, required=False)


def plan_section_search(workload: Mapping[str, Any], index: int, rng: random.Random) -> Phase:
    steps = []
    for source, token in workload.get("route_tokens", {}).items():
        steps.extend(
            [
                _activate(
                    f"park-before-{source}",
                    "Queue",
                    dispatch=rng.choice(("ax", "px")),
                ),
                _activate(
                    f"open-{source}",
                    str(source),
                    dispatch=rng.choice(("ax", "px")),
                ),
                _hover(f"hover-sample-{source}"),
                _type(f"search-{source}", SEARCH_ENTRY_LABEL, str(token)),
                Step(
                    f"clear-search-{source}",
                    "press",
                    LabelMatcher(exact=(SEARCH_ENTRY_LABEL,)),
                    {"key": "escape"},
                ),
            ]
        )
    for source in workload.get("unsupported", []):
        steps.append(_activate(f"unsupported-{source}", str(source)))
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
        steps.append(_activate("open-radio-offline", "Radio"))
        steps.append(
            Step(
                "wait-for-offline-status",
                "wait",
                fields={"duration_ms": 500, "expect_status": True},
            )
        )
    else:
        section = str(workload.get("section", "Music"))
        steps.append(_activate("restart-section", section))
        token = workload.get("search_token")
        if token:
            steps.append(_type("restart-search", SEARCH_ENTRY_LABEL, str(token)))
    steps.append(
        Step(
            "restart-app",
            "restart",
            fields={"reason": str(workload.get("reason", "Verify session restoration"))},
        )
    )
    return Phase("restart", index, tuple(steps), order_locked=True)


def plan_offline_transition(
    workload: Mapping[str, Any], index: int, rng: random.Random
) -> Phase:
    sources = list(workload.get("source_tokens", {}))
    online_sources = list(sources)
    rng.shuffle(online_sources)
    token_by_source = workload.get("source_tokens", {})
    steps = [
        _activate(
            f"online-{source}",
            source,
            token_hint=str(token_by_source.get(source, "")),
            dispatch=rng.choice(("ax", "px")),
        )
        for source in online_sources
    ]
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
    steps.extend(
        _activate(
            f"offline-{source}",
            source,
            token_hint=str(token_by_source.get(source, "")),
            dispatch=rng.choice(("ax", "px")),
        )
        for source in sources
    )
    steps.append(
        Step(
            "retry-offline",
            "activate",
            LabelMatcher(contains=("retry",)),
            {"dispatch": "ax", "expect_effect": "required"},
        )
    )
    steps.append(
        Step("go-online", "set-connectivity", fields={"connectivity": "online"})
    )
    steps.extend(
        _activate(
            f"recovery-{source}",
            source,
            token_hint=str(token_by_source.get(source, "")),
            dispatch=rng.choice(("ax", "px")),
        )
        for source in sources
    )
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
            _type("find-writable-batch", SEARCH_ENTRY_LABEL, "WRITABLE_BATCH"),
            Step("focus-first-row", "activate", ROW_MATCHER, {"dispatch": "ax"}),
            Step("anchor-down", "scroll", fields={"direction": "down", "amount": 1, "by": "page"}),
            Step("anchor-up-before-edit", "scroll", fields={"direction": "up", "amount": 1, "by": "page"}),
            Step("position-before-edit", "scroll", fields={"direction": "down", "amount": 1, "by": "page"}),
            Step("select-all", "hotkey", fields={"keys": ["ctrl", "a"]}),
            Step("context-menu", "hotkey", fields={"keys": ["shift", "f10"]}),
            Step(
                "edit-tags",
                "activate",
                LabelMatcher(contains=("edit tags",)),
                {"dispatch": "ax"},
                alternates=(context_alternate,),
            ),
            _type("batch-genre", "Genre", str(fields.get("genre", "BATCH_GENRE"))),
            _type("batch-year", "Year", str(fields.get("year", "BATCH_YEAR"))),
            Step("hover-save", "hover", LabelMatcher(contains=("save", "apply")), required=False),
            Step("save-batch", "activate", LabelMatcher(contains=(f"save {count}", "apply")), {"dispatch": "ax"}),
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
            Step(
                f"sort-{number}-{column}",
                "activate",
                LabelMatcher(contains=(column,)),
                {"dispatch": "ax", "expect_effect": "required"},
            )
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
        steps.extend(
            [
                _activate(f"add-filter-{facet}", "Add filter"),
                Step(
                    f"choose-facet-{facet}",
                    "activate",
                    LabelMatcher(exact=(str(facet).title(),), contains=(str(facet),)),
                    {"dispatch": "ax"},
                ),
                Step(
                    f"choose-value-{facet}",
                    "activate",
                    LabelMatcher(exact=(option,), contains=(option,)),
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
        )
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
