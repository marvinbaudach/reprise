---
slug: browse-bar-drop-sort-chip
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-13
---
# Retire the "Sort" chip from the browse bar

The pill labelled **Sort** next to "+ Add filter" goes away. Sorting a track
table is done where a table is sorted: by clicking its column headers.

## Why

The chip is a second, parallel entrance to a setting the table header already
owns. Every one of the ten fields the menu offers — Title, Track number,
Artist, Album, Genre, Year, Added, Length, Rating, Play count — exists as a
column in `reprise_view::columns::track::ColumnId`, and every one of those
columns is sortable by header click. Four of them (Track number, Genre, Added,
Play count) are merely not in `DEFAULT_VISIBLE`, so reaching them costs one
step: show the column, then click its header. That is the same gesture users
already know from every other table in the app, and it keeps the sort
indicator and the sorted column in the same place.

Nothing becomes unreachable. The rule that a hidden sorting column moves the
sort to the first visible sortable free column (ux-rules, the column-editing
section) already guarantees an active sort never becomes invisible.

## Scope

A starting point, not a fence — adjacent files may be touched minimally and
named in the commit message. Stop only if the *contract* above turns out to be
wrong, not because a file is missing from this list.

- `crates/reprise-gnome/src/ui/browse/browse_sort_menu.rs` — the whole module
  (`BrowseSortMenu`, its two `SimpleAction`s, `SORT_FIELDS`, the menu model)
  and its `mod` declaration in `browse/mod.rs`.
- `crates/reprise-gnome/src/ui/browse/browse_bar.rs` — the `sort_menu` field,
  its construction, its place in the `filter_actions` box, and the public
  surface `set_on_sort_changed` / `set_on_sort_open` / `sync_sort` /
  `sort_button` plus the `#[cfg(test)]` hooks
  `activate_sort_field_for_test`, `activate_sort_direction_for_test`,
  `sort_state_for_test`.
- `crates/reprise-gnome/src/ui/track_list/track_list_sort.rs` — the three
  wiring blocks in `wire_sort_clicks` that feed and mirror the menu, and the
  `sync_sort` call inside `on_sorter_changed`.
- `crates/reprise-gnome/src/ui/strings_filter.rs` — `SORT`, `SORT_TRACKS`,
  `SORT_BY`, `SORT_DIRECTION`, and whatever re-exports them via `ui::strings`.
- `po/reprise.pot` and the seven catalogues (`ar bn de es fr hi zh_CN`), each
  of which carries four `Sort…` msgids from these constants.
- Tests: `browse/browse_bar_tests.rs` (the case around the sort button) and
  the sort tests at the end of `track_list_sort.rs`.

## Decisions (do not re-litigate)

1. **Header sorting is untouched.** `wire_sort_clicks`' `ColumnViewSorter`
   connections, `on_sorter_changed`, the `SortState` it writes, the reload it
   triggers, and the debounce that stops one click from firing two identical
   queries all stay exactly as they are. Only the menu-mirroring calls go.
2. **No replacement affordance.** No sort entry migrates into the "+ Add
   filter" popover, the hamburger menu, or a context menu. The header is the
   sort surface, full stop.
3. **Sort persistence stays.** `SortState`, `restored_sort`,
   `default_sort_for_source`, `resolve_sort_on_switch` and the view-state
   memory are independent of the menu and keep working — a restored sort still
   has to show up as the header indicator on load.
4. **`stats_songs_card.rs`'s "Sort top tracks" toggle is a different control**
   in a different view. Do not touch it or its string.
5. **Delete, don't deprecate.** No dead `#[allow(dead_code)]` shell of
   `BrowseSortMenu` is left behind, and no orphaned strings stay in
   `strings_filter.rs`.

## Work

1. Remove the sort pill from the browse bar: drop the `sort_menu` field and
   its construction, stop appending it to `filter_actions`, and delete the
   `browse_sort_menu` module and its `mod` line. If `filter_actions` now holds
   a single child, simplify it only if that changes no geometry — the filter
   bar's fixed height (QA #8) must not shift.
2. Unwire the menu in `track_list_sort.rs`: remove the `set_on_sort_changed`,
   `set_on_sort_open` and `sync_sort` blocks from `wire_sort_clicks` and the
   `sync_sort` call in `on_sorter_changed`. Check whether
   `column_for_sort_field` and `sort_by_column` still have callers (view
   restore may use them); delete only what is genuinely unused, keep what
   restore needs.
3. Retire the four strings and regenerate `po/reprise.pot` plus the seven
   catalogues with `msgmerge`, the way `scripts/tests/gettext-catalogs.sh`
   expects.
4. Tests: delete or rewrite the cases that drive the menu
   (`activate_sort_field_for_test`, `activate_sort_direction_for_test`,
   `sort_state_for_test`, the browse-bar case asserting the "Sort" label).
   The behaviour they pinned — a field/direction change lands in `SortState`
   and reloads the list — must still be covered, now driven through the
   header/`ColumnViewSorter` path instead of the menu. Do not simply drop
   coverage of sorting.
5. `docs/ux-rules.md`: grep the filter-bar (FIL) and column sections for any
   rule that names the sort control or enumerates the browse bar's zones as
   place / facets / add-filter / sort / count / clear-all, and remove the sort
   element from it. A removal does not get a new rule ID. If no rule mentions
   it, say so in the summary rather than inventing one.
6. Screenshots and docs that show the pill are historical records — do not
   rewrite closed plans or mockup transcripts.

## Verification

- `cargo test -p reprise-gnome --bin reprise` (there is no `--lib` target in
  this crate; `--lib` runs nothing).
- `scripts/tests/gettext-catalogs.sh` for the catalogue regeneration.
- Do not open an app window on the real desktop for a visual check; if visual
  evidence is wanted, use the repo's headless screenshot harness.
