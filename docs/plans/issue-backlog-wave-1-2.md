---
slug: issue-backlog-wave-1-2
worktree: /home/marvin/Projects/reprise-issue-backlog-wave-1-2
branch: feature/issue-backlog-wave-1-2
phase: planned
codex_session:
created: 2026-08-22
---
# Strand 2 — #404: sorting becomes reachable without a mouse

Mother plan: `docs/plans/issue-backlog-wave-1.md`. Base `origin/dev` = `1515487599`.
Sweep task 5 (`docs/plans/open-issue-sweep-2026-08.md`).

This strand owns, and writes **only**:

```
crates/reprise-gnome/src/ui/browse/**
crates/reprise-gnome/src/ui/track_list/track_list_sort.rs
crates/reprise-gnome/src/ui/strings.rs
docs/ux-rules.md
docs/plans/issue-backlog-wave-1-2.md
```

`crates/reprise-gnome/src/ui/track_list/track_list.rs` is written by **nobody** in
this wave. Task 2's design exists to keep it that way — read `shared.sort` when
the popover opens instead of adding an observer to `Shared`.

---

## The finding this strand acts on

Sorting the track table has exactly one control today: clicking a column header
(`crates/reprise-gnome/src/ui/track_list/track_list_sort.rs:61-114`,
`wire_sort_clicks` → `on_sorter_changed`). The header widget is a
`GtkColumnViewTitle`, which reports AT-SPI role `filler` and carries no action,
and GTK offers no API to change that. So for assistive technology the table
cannot be sorted at all — 52 occurrences over both seeds of the sweep that filed
#404.

The GTK defect gets its own upstream report; that is **not** part of this strand.
What this strand does is give sorting a second, properly labelled control that
writes the same state.

## Task 1 — the rule first

`docs/ux-rules.md` today covers column behaviour under **STYLE-13** [active]
("Columns belong to the user, in every table"), which describes the header sorter
and the header-band column editor. Two general accessibility rules — **GP-1** and
**GP-10** — are `[planned]`, not `[active]`.

Write the contract before building it: extend STYLE-13 (or add a rule adjacent to
it, whichever fits that document's shape) to say that **sorting is reachable
without a pointer**: the sort field and direction are available from a labelled
control in the browse bar, that control and the column header write the same sort
state, and neither is a second source of truth. Name the issue in the rule text
the way this document names issues elsewhere.

If the new behaviour makes GP-1 or GP-10 partly true, do **not** flip their status
in this strand — a `[planned]` rule becomes `[active]` when it holds everywhere,
not when one more widget complies.

## Task 2 — the control

Add a sort `MenuButton` to `crates/reprise-gnome/src/ui/browse/browse_bar.rs`,
beside the existing `add_filter` button (`browse_bar.rs:99-110`).

Shape it after `add_filter`, which is the local idiom: a `gtk4::MenuButton` with
a label child, the `pill` style class, styling delegated to
`filter_bar_layout`, and an explicit accessible label via `update_property`
(`browse_bar.rs:108-110`). It must match `+ Add Filter` in height and style
classes — it sits next to it and a mismatched twin is worse than no twin.

Contents: the sort **field** and the sort **direction**. The fields are the ones
the table already accepts — derive them from the existing whitelist
(`queries::SORT_WHITELIST` / `ColumnId::from_sort_field`) rather than typing a
second list; a hand-kept copy will drift the first time a column is added.

Labels come from `crates/reprise-gnome/src/ui/strings.rs` through the module's own
`text`/`formatted`/`plural` helpers and the `N_!` macro, never from literals in
the widget code. Reuse the column-header strings where they already exist rather
than inventing a second wording for the same field.

**One truth.** Choosing an entry must write `shared.sort` and trigger the reload
through the path that already exists — `track_list_sort.rs`'s
`sort_by_column(view, column, order)` (`track_list_sort.rs:50-57`) drives the
`ColumnView`'s own sorter, whose change observer then writes `shared.sort` and
calls `reload`. Go through that, so a menu choice and a header click are the same
event downstream. Do not write `shared.sort` from the browse bar directly, and do
not call `reload` twice.

**The mark, without a new observer.** The menu shows which field and direction are
current. Read `shared.sort.borrow()` when the popover is about to be shown and set
the marks then. `Shared` has no change signal for `sort` and this strand must not
add one — a popover that is closed has nothing to display, so reading on open is
both sufficient and simpler. Note this reasoning in a comment so the next reader
does not "fix" it into an observer.

There is no radio-style `GMenu` precedent in this codebase — the only stateful
action is a boolean in
`crates/reprise-gnome/src/ui/compact/compact_player_menu.rs:74-89`, and
`gio::ActionEntry` is unused repo-wide. Either build the radio group with
`gio::SimpleAction::new_stateful` carrying a string state, or build the popover
the way `add_filter` does — a plain `gtk4::Popover` with list rows. Pick the one
that yields a **labelled, activatable** element per choice in the accessibility
tree; that is the whole point of the change. Whichever you pick, every row or menu
item carries an accessible label and the current choice is exposed as state, not
only as a drawn checkmark.

## Task 3 — keyboard reach

A control that only a pointer can open does not discharge this issue. The new
button must be reachable by keyboard on the same tab path as `+ Add Filter`, its
popover navigable with the arrow keys, and closable with Escape. If `add_filter`
already sits in a working tab order, place the new button so it inherits it
rather than building a second focus model.

## Acceptance

The claim to be proven is "sorting is reachable without a pointer, and there is
still only one sort state" — not "a menu was added".

1. **A displayless test that a menu choice and a header click converge.** Drive
   the new control's activation handler for a given field and direction, and
   assert `shared.sort` afterwards equals what the header click produces for the
   same column — and that `reload` ran exactly **once**. The existing display test
   `column_headers_update_sort_state_and_reload_once`
   (`track_list_sort.rs:391-454`) already counts reloads; follow its shape. If the
   handler cannot be reached without a display, make it a display test.
2. **The list does not drift.** A test that the menu's field list is exactly the
   accepted sort whitelist. It must fail if a field is added to one and not the
   other.
3. **The a11y shape.** Assert what this GTK binding permits: `accessible_role()`
   readback works (see `track_list_columns_tests.rs:71,95,148` for the idiom),
   `Property::Label` has no getter in gtk4-rs 0.11.4. So assert the roles and the
   activatability of each choice, and cover the labelling through
   `scripts/check-accessibility-semantics.sh` — extend it if the new widget's
   shape is not already covered, and record a red run with the labels removed and
   a green run with them present.
4. **Mutation proof for the single-truth claim.** Make the menu write
   `shared.sort` directly instead of going through the sorter, run the tests, and
   record that the "reload ran once" assertion goes **red**. Revert. A test that
   stays green under that mutation is not testing the claim.
5. **Nothing regressed in the browse bar.** `browse_bar_tests.rs` currently has 20
   tests; run them and record the count before and after.
6. Run the displayless GNOME suite (`cargo test --locked -p reprise-gnome`, fresh
   XDG roots, `REPRISE_AUDIO_SINK=fakesink`) and record passed/failed.

**Not in this strand:** the upstream GTK report about `GtkColumnViewTitle`, any
change to the header sorter's own behaviour, and #411's busy indicator — which
lands in the same file later and must not be anticipated here.

---

## Result

*(Codex fills this in: commands, counts, the mutation proofs with their red and
green runs, and anything that could not be proven and why.)*
