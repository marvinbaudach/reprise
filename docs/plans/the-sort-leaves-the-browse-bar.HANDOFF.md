# The sort leaves the browse bar — session handover

Session of 2026-08-27/28, landed as PR #725 against `dev` on 2026-08-28. The
branch `feature/the-sort-leaves-the-browse-bar` was finished, reviewed,
refactored and measured before it was opened. This file exists so the reasoning
survives the session.

Branch: four commits on `origin/dev` @ `3000da8cd2`, plus this one.

    ac3754495b  docs: plan
    08c70ef3b4  feat(gnome): move sorting into table customization
    2691fb8704  fix(gnome): address table sorting review findings
    a0826c8948  docs: AT-SPI evidence run

## What the change is

Sorting stopped being a pill in the track list's browse bar and joined the
shared "Customize table…" surface, which renders into both the primary-menu
dialog and the header-band right-click popover. `EditorModel` grew three
defaulted methods (`sortable_columns`, `sort`, `set_sort`); `ColumnRegistry<K>`
implements them once for every table. A new `TrackListEditorModel` wrapper keeps
"Playlist order" restorable, which it was not before — once a playlist was
sorted by a column the only way back was to leave and re-enter it.

`BrowseSortControl` is gone (280 lines). `EDIT_COLUMN_LAYOUT` became
`CUSTOMIZE_TABLE`; the msgid "Sort" was retired, "Playlist order" added.

## Two things the plan got wrong, corrected in the branch

**Radio has no sortable columns at all.** The plan claimed Releases, Concerts
and Radio all have "the same pointer-only headers" and expected the evidence run
to find sort radios in all four tables. There is no `set_sorter` anywhere in
`crates/reprise-gnome/src/ui/radio/` — verified by grep. Radio therefore has no
sorting today, not even with a pointer, and `sortable_columns()` correctly
returns empty for it. The chosen resolution was to correct the prose, not to
wire sorters onto Radio columns: that would be a feature outside the plan. The
plan text and its Verification section were amended accordingly.

**The msgid arithmetic was wrong.** Task 8 claimed "Net +1 msgid". It is ±0 —
1,307 before and after. Two retired ("Edit column layout…", "Sort"), two added
("Customize table…", "Playlist order"). Codex flagged this itself rather than
quietly matching the plan.

One assumption the plan told us to *check* rather than assume held: the
sorter-bearing track-list columns and the fields `ColumnId::from_sort_field`
accepts coincide exactly, so no intersection fallback was needed.

## Review

Three reviewers (two Rust, one for docs/gettext). Ten findings accepted and
applied by Codex in `2691fb8704`. The two that mattered:

**A reference cycle in `build_sort_section`.** Field-choice closures captured
`descending` strongly; the direction closures captured an `Rc` holding every
field button. GTK-rs owns a signal closure from the object it is connected to,
so `descending → field buttons → descending` was closed: `dispose` never ran,
and `present_dialog` rebuilds the section on *every* activation, so each opening
of "Customize table…" leaked a cluster of widgets and the captured model. Now
weak on both sides, matching what `build_row` in the same file already did.

The two Rust reviewers disagreed here — one reported the cycle, one explicitly
cleared it, having only considered whether the *model* held a back-reference and
not the widget-to-widget edge. Reading the code settled it in favour of the
finding. Worth remembering that a "no finding" from a reviewer is not symmetric
with a finding: it is only as good as the edge it thought to look for.

**Two tests that had stopped proving their own names.** The moved
`style_13_sort_choices_are_keyboard_radio_actions` had been flattened into one
list and lost its per-group "exactly one active" assertion; the
`…converge_and_reload_once` test snapshotted the reload counter *after*
`restore_playlist_order` had already run, so the one write path outside the
`ColumnViewSorter` observer went unmeasured. Both restored.

The M1 regression test was verified non-vacuous by temporarily restoring the
strong cycle and confirming it fails.

## Evidence run

See `the-sort-leaves-the-browse-bar.EVIDENCE.md`; the raw JSON trees behind it
are not committed, and the note closing that file says why and where they are.
Summary: the browse-bar pill's single AT-SPI action is gone; eight labelled,
focusable radio buttons carrying `checked` state took its place; the column
headers expose no action *in both arms*, which is what proves the probe was
reading the same thing before and after.

Three limits are recorded there rather than papered over: the F10 route itself
was not proven (neither key injection nor the AT-SPI click action opens a GTK
popover under Xvfb — the surface was opened through the same GAction the menu
entry invokes); only the Music table was reachable; and the radios expose no
Action interface, which is unchanged from the deleted pill and therefore not a
regression.

## Traps worth not rediscovering

- **`REPRISE_SMOKE_FIRST_RUN=setup-options` does not enable the source
  modules**, despite mapping to `WizardSourceSelection::EnableAll`. Measured:
  the wizard logs "first-run setup completed", and `online-sources-enabled`
  stays `0` with no `module.*` row written. The modules had to be set directly
  in the `settings` table (`module.<id>.enabled` = `1`, plus
  `online-sources-enabled`).
- **`REPRISE_SMOKE_QUIT` is a flag, not a duration** — the delay lives in
  `REPRISE_SMOKE_QUIT_DELAY_SECS` (default 3). And when its timer fires it logs
  "closing main window" but the process keeps running, so every headless launch
  needs its own `timeout`.
- **The sidebar exposes no named AT-SPI rows.** Not "Releases", not "Radio", not
  even the ungated "My Stats" — the tree carries a single `label` "Music". There
  is no GAction, env hook or D-Bus handle for the top-level page either, so
  nothing outside the app can select one. This is not caused by this change and
  looks worth its own investigation.
- **GTK window actions are readable on the session bus**, which is the cheapest
  handle for exactly this kind of question: `org.gtk.Actions.DescribeAll` on
  `/io/github/marvinbaudach/Reprise/window/1` reports every action *and whether
  it is enabled*. That measures "the menu entry stays insensitive" at the source
  instead of inferring it from pixels.
- **AT-SPI role names are not what you assume.** This version reports `button`,
  not `push button`; a `GtkMenuButton` publishes *two* nodes with the same name,
  only one of which carries the action. A probe that takes the first match
  silently activates nothing.
