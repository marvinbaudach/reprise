---
slug: the-sort-leaves-the-browse-bar
worktree: /home/marvin/Projects/reprise-the-sort-leaves-the-browse-bar
branch: feature/the-sort-leaves-the-browse-bar
phase: planned
codex_session:
created: 2026-08-27
---
# The sort leaves the browse bar

Sorting stops being a pill in the track list's browse bar and joins the surface
that already answers "how is this table arranged" — reached by right-click on the
header band and from the primary menu, in every table that has columns.

## What is actually true today

The premise that started this ("the sort button is unnecessary, you can sort in
the table") is true for a pointer and false for everything else.

- **The column header row exposes no accessible action.** Issue #404, closed:
  *"sorting by column is unreachable for assistive technology. Reproducibility:
  2 runs, 1 mission, both seeds, 52 occurrences."* The browse-bar control was the
  fix, and `docs/ux-rules.md:3392` (STYLE-13, `[active]`) records it: *"sorting is
  also reachable without a pointer through a labelled control in the browse bar."*
- **The control is not a second sort state.** `browse_sort.rs:227-238` routes
  every choice through `track_list_sort::sort_by_field`; the `ColumnViewSorter`
  observer stays the only writer of `Shared::sort` (`track_list_sort.rs:60,80`).
- **Its field list is generated from the table's own columns**
  (`browse_sort.rs:194-205`), so it offers no criterion a column does not have.
  `browse_bar_tests.rs:563` freezes that 1:1.
- **#404 was closed for one table only.** Releases, Concerts and Radio have the
  same pointer-only headers (`releases_view.rs:685-690`,
  `concerts_view.rs:761-786`) and no browse-bar control at all. For assistive
  technology, sorting is unreachable there today. Nobody has measured it, so it
  is not a known-red rule — it is an unmeasured gap of the same shape.
- **One real defect:** `sortable_columns()` has no visibility filter, so the menu
  offers sorting by columns the user has permanently hidden in preferences.
  `ColumnRegistry::layout()` (`table_columns/registry.rs:252`) is the correct
  discriminator: the narrow-window breakpoint writes the GTK `visible` property
  directly (`responsive_columns.rs:49-55`) and never touches `current_layout`, so
  the layout still says what the *user* chose while a fold is in effect.

So the button cannot simply be deleted, and it cannot be hidden above 760px
either — that would restore #404 in the normal desktop case. What it can do is
move somewhere that serves every table instead of one.

## The surface it moves into

`win.edit-column-layout` already solves this problem, and it solves it twice over
from a single model:

- `EditorModel` (`table_columns/descriptor.rs:15-27`) — string-id based:
  `title`, `columns() -> Vec<ColumnDescriptor>`, `is_visible`, `set_visible`,
  `move_column`, `reset`. **Two production implementations exist**: the generic
  `impl<K: ColumnKey> EditorModel for ColumnRegistry<K>` (`registry.rs:367`) and
  the Concerts wrapper `LocationAwareEditorModel`
  (`concerts_location_columns.rs:16`). All four tables hang off the first.
- `editor::build_surface(model, show_window_controls)` (`editor.rs:138`) builds
  one surface — a header bar with Reset and `model.title()` (`editor.rs:157-161`)
  over a `ScrolledWindow` around the column list (`editor.rs:162-168`).
- That surface is rendered **twice**: into an `adw::Dialog` presented from the
  menu action (`editor.rs:197-229`, `primary_menu.rs:157-173`) and into the
  right-click popover on the header band (`header_popover.rs:17-38`,
  `BUTTON_SECONDARY`). `show_window_controls` differs; the content does not.
- `ActiveTable` resolves the page name to a model in one `match`
  (`window/table_columns.rs:60-66`), re-run on
  `connect_visible_child_name_notify` and `connect_visible_page_notify`
  (`table_columns.rs:72-78`); sensitivity is `model.is_some()`
  (`table_columns.rs:115-128`).

Sorting joins that surface. It therefore inherits both a discoverable pointer
path and a keyboard/AT path without a second action, a second dialog or a second
`ActiveTable` consumer — and it reaches all four tables through one generic
implementation.

## Design

### 1. `EditorModel` grows three sort methods, with default implementations

```rust
fn sortable_columns(&self) -> Vec<ColumnDescriptor> { Vec::new() }
fn sort(&self) -> Option<(String, gtk4::SortType)> { None }
fn set_sort(&self, _id: &str, _order: gtk4::SortType) {}
```

`gtk4::SortType` rather than a new enum: the registry already speaks it
(`registry.rs:180-186`), so no conversion layer appears. The `"asc"`/`"desc"`
strings stay where they already live, in the track list's own persistence.

Defaults, not required methods, because five test fakes implement this trait
(`primary_menu.rs:283`, `window/table_columns.rs:136`, `editor.rs:252`,
`header_popover.rs:82`, `descriptor.rs:38`) and none of them are about sorting.
Required methods would add fifteen stubs to tests that do not care. Both
production implementations override all three, so nothing real falls back to the
default.

### 2. One implementation covers every table

`ColumnRegistry<K>` implements the three methods:

- `sortable_columns()` — the columns in current order that are **visible in
  `layout()`** and carry a sorter. Visible-in-layout, not `is_visible()`
  (`registry.rs:226-229`), which reads the live GTK property and would drop
  everything the narrow fold has hidden. This single filter is the
  hidden-column defect fixed, for all four tables at once.
- `sort()` — the `ColumnViewSorter`'s primary column and order, so the surface
  always opens marked with what the headers actually show.
- `set_sort()` — `view.sort_by_column(column, order)`.

**The track list's single writer stays single.** The registry already sorts that
view this way in its own sort fallback (`registry.rs:172-186`); the
`ColumnViewSorter` observer sees the change and remains the only writer of
`Shared::sort` and the only reload trigger (`track_list_sort.rs:60,80`). No
per-view sort implementation is written, and `sort_by_field` is not called from
the surface.

`LocationAwareEditorModel` forwards the three methods to the registry it wraps.

### 3. Playlists get their order back

In a playlist the sort field is the sentinel `playlist_order`
(`track_list_sort.rs:141,166`), which has no column — so `sort()` would return
`None` and the surface would show an unmarked radio group in every playlist.

A track-list wrapper around the registry, built on the same pattern as
`LocationAwareEditorModel`, prepends a synthetic first descriptor ("Playlist
order") while the view source is a playlist, reports it as the current field when
the sentinel is active, and on `set_sort` restores the sentinel through
`track_list_sort` instead of the `ColumnView`. It is wired into the `"library"`
arm of `active_table()` (`window/table_columns.rs:60-66`).

Consequence beyond the fix: the manual order becomes deliberately restorable.
Today, once a playlist is sorted by a column, the only way back is to leave the
playlist and re-enter it, where `resolve_sort_on_switch` forces the sentinel
(`track_list_sort.rs:209`).

### 4. The section, and what the surface is called

A "Sort by" radio group over `sortable_columns()` plus an
ascending/descending group, placed above the column list **inside** the existing
`ScrolledWindow` so neither the dialog nor the 360×440 popover
(`header_popover.rs:19-22`) needs to be resized. Labelled, keyboard navigable,
exposing the current choice as state — the properties STYLE-13 already demands,
carried over from `browse_sort.rs:29-46` together with its two
`// a11y-semantics:` markers, whose format
`scripts/check-accessibility-semantics.sh` enforces. The whole section is hidden
when `sortable_columns()` is empty.

The surface now carries sorting as well as columns, so "Edit column layout…"
becomes **"Customize table…"** — one menu entry (`primary_menu.rs:57-69`) and one
title, since `model.title()` for the registry is that same string
(`registry.rs:369`).

### 5. The browse bar loses the pill

`BrowseSortControl` is deleted (`browse/browse_sort.rs`, ~250 lines), together
with the `sort_control` field (`browse_bar.rs:58`), its construction
(`browse_bar.rs:96`) and its packing into `filter_actions`
(`browse_bar.rs:118`). `SORT_BY`, `SORT_DIRECTION`, `SORT_ASCENDING` and
`SORT_DESCENDING` (`strings.rs:409-413`) move to the section and keep their
msgids. `SORT` ("Sort", used only by the pill) is retired.

## Tasks

1. **Trait** (`table_columns/descriptor.rs`): the three defaulted methods above.
   Extend that file's `Fake` and its `mod tests` to cover the defaults. The tree
   stays green with nothing else changed.
2. **Generic implementation** (`table_columns/registry.rs`): `sortable_columns`
   from `layout()` (:252) and `column()` (:222) plus a has-sorter check, `sort`
   from the `ColumnViewSorter`, `set_sort` via `sort_by_column`. The has-sorter
   predicate is the one this file already uses in its sort fallback
   (`column.sorter().is_some()`, `registry.rs:302`) — not a new invention.
   Unit-test the visibility filter: a column hidden in the layout is not
   offered; a column hidden only by the narrow fold still is.

   **One assumption must be checked here, not assumed.** The old pill filtered
   on a whitelist (`ColumnId::from_sort_field`, `browse_sort.rs:194-205`) and the
   track list's sorter observer rejects any field outside it
   (`track_list_sort.rs:35`). The registry filters on "carries a sorter". If a
   track-list column carries a sorter but is not whitelisted, the section would
   offer a sort the observer then refuses — silently. Add a test asserting the
   two sets coincide for the track list; if they do not, the offered set is
   their intersection and the test says so.
3. **Concerts wrapper** (`concerts/concerts_location_columns.rs`): forward the
   three methods.
4. **Playlist wrapper** (new, `track_list/`): synthetic "Playlist order"
   descriptor while the source is a playlist, sentinel restore on `set_sort`,
   wired into the `"library"` arm of `active_table()`
   (`window/table_columns.rs:60-66`). Unit-test the descriptor list and the
   sentinel round-trip without GTK.
5. **The section** (`table_columns/editor.rs`): build it inside `build_surface`
   above the list, hidden when `sortable_columns()` is empty, a11y markers
   carried over. It appears in dialog and popover by construction — assert both.
6. **Rename**: `EDIT_COLUMN_LAYOUT` → `CUSTOMIZE_TABLE = N_!("Customize table…")`
   at `strings.rs:234`, its uses at `primary_menu.rs:65`, `registry.rs:369` and
   `preferences/preference_layout.rs:433`. The ellipsis follows the convention of
   `EDIT_TAGS` and `IMPORT_PLAYLIST` (`strings.rs:239,648`). The preferences use
   is an `action_row` opening the same surface, so the rename carries — but it
   sits under a group titled "Columns" (`visual_strings::COLUMNS`). Check that
   the row still reads sensibly under that heading and say so; do not leave a
   silent mismatch.
7. **Delete `BrowseSortControl`** and its packing; move
   `style_13_sort_choices_match_every_accepted_table_sort_field`,
   `style_13_sort_choices_are_keyboard_radio_actions` and
   `style_13_sort_popover_closes_on_escape` (`browse_bar_tests.rs:457,563,602`)
   to the editor's test module **under their existing names**, retargeted at the
   section. Also remove the `wire` call near `track_list_builder.rs:371`.
8. **Strings**: retire `SORT`, add `CUSTOMIZE_TABLE` and a "Playlist order"
   msgid, regenerate `po/reprise.pot`, update all eight `po` files. Net +1 msgid.
9. **STYLE-13** (`docs/ux-rules.md:3372-3421`): rewrite the #404 sentence — the
   pointer-free path is the table-customization surface, reached from the primary
   menu and from right-click on the header band, and it covers every table with
   sortable columns, not only the music library. Update the **Test rule** name
   list to where the tests now live. `scripts/check-ux-traceability.sh` fails on
   any listed name no test carries, so tasks 7 and 9 land together or not at all.
10. **Evidence run** (below). Not a code task; it is what makes the change
    provable rather than merely plausible.

## Verification

- `cargo test -p reprise-gnome` — trait defaults, the visibility filter, the
  playlist descriptor and sentinel round-trip, `ActiveTable` resolution.
- Display tests through `scripts/check-display-tests.sh`: the moved
  `style_13_sort_*` tests keep `#[ignore = "requires a display; run via
  xvfb-run"]` and `crate::ui::test_main_context::lock_main_context()`, per
  `browse_bar_tests.rs`.
- **AT-SPI probe with a control arm**, the same measurement kind that found #404:
  - *Control arm, before the change:* the header row exposes no action in any of
    the four tables; the sort pill exposes one, in the Library only.
  - *After:* the header row still exposes no action — unchanged, so the probe is
    reading the same thing — and the path F10 → "Customize table…" → sort radios
    exposes one in Library, Releases, Concerts and Radio, and stays insensitive
    in Podcasts, YouTube and Stats.
  - Record the tree, not a screenshot.
- `scripts/check-ux-traceability.sh` — green after task 9; the gate that proves
  tasks 7 and 9 stayed in step.
- `scripts/check-accessibility-semantics.sh` — marker format on the moved widgets.
- `scripts/check-input-parity.sh`, `scripts/check-gnome-idioms.sh` and
  `scripts/ci-quality.sh` as the umbrella.

## Out of scope

- **Podcasts and YouTube.** They resolve to `None` in `ActiveTable`, asserted by
  `style_13_only_table_pages_resolve_an_editor_model`
  (`window/table_columns.rs:156-172`); there is no `youtube` UI module and the
  podcasts view builds no sortable columns. They stay insensitive, exactly as
  they are for the column editor today.
- **The narrow-window fold.** `responsive_columns.rs` keeps its one-shot toast.
  Once sorting lives in the shared surface it is reachable at any width, which
  removes the reason to touch the fold.
- **A full #404-style CUA sweep** across all four tables. The gap in Releases,
  Concerts and Radio is argued from the same code shape, not measured; the AT-SPI
  probe above proves the new path works, not the size of the old gap. Worth its
  own exploratory run.

## Parallelität

**No cut. Single strand.**

Every task depends on task 1, and tasks 1 and 2 are two files — `descriptor.rs`
and `registry.rs` — that everything downstream imports. A second strand would
have to author the same trait signature and the same generic implementation to
compile at all, which is an add/add conflict on exactly the lines that define the
contract.

The seam that looks plausible — "track list here, the other tables there" — does
not exist any more. It existed in the draft, which assumed one sort
implementation per view; the single generic `impl EditorModel for
ColumnRegistry<K>` collapsed those into one. What is left per table is the
Concerts wrapper's three forwarding methods (task 3) and the playlist wrapper
(task 4), both smaller than the cost of a worktree and a merge.

Tasks 7, 8 and 9 must land in the same commit range as each other:
`check-ux-traceability.sh` reads the rule's test-name list against the tests that
exist, so deleting `BrowseSortControl` without amending STYLE-13 turns the gate
red, and amending STYLE-13 without moving the tests does the same in the other
direction. Task 6 belongs with them: the rename touches the same msgid set as
task 8.

There is no merge order and there are no post-merge cross-checks, because there
is nothing to merge across.
