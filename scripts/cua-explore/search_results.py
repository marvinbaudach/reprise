"""Which elements of a snapshot are the results a search left on screen.

cua-driver 0.33 gives a sidebar entry, the column-header row, a table row and a
source card nearly the same shape: each is a `list item`, a `row` or a `button`,
and `ui_vocabulary` folds the list item onto `row`. Counting every row therefore
counted `Music`, `Queue` and the rest of the sidebar, plus the header, and left
the podcast and YouTube results - plain buttons - out altogether.

What separates them is where they sit, which the snapshot does state: every
element names its parent. The window's direct children are the chrome and the
sidebar entries; the one `group` among them is the page, and everything the
search can leave behind lives under it. Inside the page a result is

- a data row: a `row` whose parent is a `list`. The column-header row hangs off
  the table itself, so it is not one; or
- a text-only button: a `button` that has no action and no toggle child. Every
  control on a page is a `button` that carries a click, or a GTK menu button -
  a `button` wrapping a `toggle button` - so what is left is an item the user
  reads, such as an episode inside a source card.

Without parent links the position is unknown. The helpers then say so
(`None`) and `result_elements` falls back to the rows of the observation.
"""

from __future__ import annotations

from typing import Any, Mapping, Sequence

from ui_vocabulary import canonical_role, invocable_actions, is_row

PAGE_ROLE = "group"
DATA_ROW_PARENT_ROLE = "list"
TEXT_ITEM_ROLE = "button"
MENU_BUTTON_CHILD_ROLE = "toggle button"
CARD_ACTION = "activate"


def _action_tokens(item: Mapping[str, Any]) -> tuple[str, ...]:
    return tuple(str(name).strip().casefold() for name in item.get("actions") or ())


def _index(item: Mapping[str, Any]) -> int | None:
    value = item.get("element_index")
    return value if isinstance(value, int) and not isinstance(value, bool) else None


def page_items(
    raw_elements: Sequence[Mapping[str, Any]],
) -> tuple[frozenset[int], frozenset[int]] | None:
    """The indices of the results and of the source cards on the page.

    None when the snapshot has no parent links. A source card is the other kind of
    button the page can hold twice: it carries an `activate` action, which makes it
    no result (an episode inside it is) but still a source that must be listed once.
    """
    by_index = {
        index: item for item in raw_elements if (index := _index(item)) is not None
    }
    if not by_index or not any("parent_index" in item for item in by_index.values()):
        return None

    def parent(item: Mapping[str, Any]) -> Mapping[str, Any] | None:
        return by_index.get(item.get("parent_index"))

    def branch(item: Mapping[str, Any]) -> Mapping[str, Any] | None:
        """The ancestor that is a direct child of the window."""
        seen = 0
        while seen <= len(by_index):
            seen += 1
            above = parent(item)
            if above is None:
                return None
            if parent(above) is None:
                return item
            item = above
        return None

    toggled = {
        item.get("parent_index")
        for item in by_index.values()
        if canonical_role(str(item.get("role", ""))) == MENU_BUTTON_CHILD_ROLE
    }
    results = set()
    cards = set()
    for index, item in by_index.items():
        top = branch(item)
        if top is None or canonical_role(str(top.get("role", ""))) != PAGE_ROLE:
            continue
        role = canonical_role(str(item.get("role", "")))
        above = parent(item)
        if is_row(role):
            if above and canonical_role(str(above.get("role", ""))) == DATA_ROW_PARENT_ROLE:
                results.add(index)
        elif (
            role == TEXT_ITEM_ROLE
            and not invocable_actions(item.get("actions") or ())
            and index not in toggled
        ):
            results.add(index)
        elif role == TEXT_ITEM_ROLE and CARD_ACTION in _action_tokens(item):
            cards.add(index)
    return frozenset(results), frozenset(cards)


def result_indices(raw_elements: Sequence[Mapping[str, Any]]) -> frozenset[int] | None:
    """Indices of the result elements, or None when the snapshot has no parent links."""
    found = page_items(raw_elements)
    return None if found is None else found[0]


def source_card_indices(raw_elements: Sequence[Mapping[str, Any]]) -> frozenset[int] | None:
    """Indices of the source cards, or None when the snapshot has no parent links."""
    found = page_items(raw_elements)
    return None if found is None else found[1]


def result_elements(observation: Mapping[str, Any]) -> list[Mapping[str, Any]]:
    """The labelled result elements of an observation, in observation order."""
    elements = [
        item
        for item in observation.get("elements", [])
        if isinstance(item, Mapping) and item.get("label")
    ]
    if any("result" in item for item in elements):
        return [item for item in elements if item.get("result")]
    return [item for item in elements if is_row(str(item.get("role", "")))]


def source_cards(observation: Mapping[str, Any]) -> list[Mapping[str, Any]]:
    """The labelled source cards of an observation; none when the page is unknown."""
    return [
        item
        for item in observation.get("elements", [])
        if isinstance(item, Mapping) and item.get("label") and item.get("source_card")
    ]
