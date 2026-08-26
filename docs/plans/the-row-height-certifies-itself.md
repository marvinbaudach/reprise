---
slug: the-row-height-certifies-itself
worktree:
branch:
phase: planned
codex_session:
created: 2026-08-26
strands: a,b
merge_order: a,b
---
# The row height certifies itself

The track table's row height is decided by a value the same code wrote. Three
heights have now certified themselves in a row — 53, then 45, then 30 — and each
one was persisted as "measured". This plan removes the loop rather than the
number.

Predecessor: `docs/plans/the-table-follows-the-music-again.md` (strands
`7c3dfcc10a` #693 and `8bd2d23930` #694, cross-check record `825df8ccef` #696).
Its handoff, `docs/plans/the-table-follows-the-music-again.HANDOFF.md`, reported
the feature still broken after both strands landed. Two of that handoff's
findings are superseded here — both marked below.

Strands: [`-a`](the-row-height-certifies-itself-a.md) (the loop and everything
that reads it), [`-b`](the-row-height-certifies-itself-b.md) (the persisted
number and the record).

---

## 1 — What the user sees

Stepping the transport reveals the playing row, but the viewport lands **short,
proportionally to how far down the list the row sits**. Near the top it is a
nudge; at row 1900 of 2006 it is hundreds of rows. Reported as "not fully
centred".

## 2 — Root cause, in one sentence

`ListGeometry::configure` writes `upper` from the remembered row height, and
`remember_if_settled` reads `adjustment.upper()` back as if it were GTK's own
measurement — so any remembered height certifies itself on the next pass and is
persisted as measured.

Nothing in the code distinguishes *"GTK wrote this upper"* from *"we wrote this
upper"*. That distinction is the entire bug, and restoring it is the whole plan.

## 3 — The chain, measured

Evidence: `~/.local/share/reprise/diagnostics/table-follows-2026-08-25/reveal-still-broken-trail-2026-08-25.txt`
(479 lines, `REPRISE_SCROLL_PROBE=1 REPRISE_DEBUG_SCROLL=1`, installed nightly of
`825df8ccef`, 2006 rows, `page=805`), plus the live database.

1. **The database holds 30.** `sqlite3 ~/.local/share/reprise/reprise.db` →
   `ui.row_height|30`, `user_version=79`. (The handoff's "`ui.row_height` is back
   to `53`" is **obsolete** — superseded here.) `ui.section_header_height` does
   not exist in that database at all.

2. **We overwrite GTK's number with it.** `configure` → `preseed_upper` →
   `adjustment.configure(upper = 2006 × 30 = 60180)`. The trail shows this
   **six times and never once the other way**:

   ```
   SCROLLUPPER writer=anchor.configure want=60180.0 from=90270.0     (x6)
   ```

   `90270 = 2006 × 45` is GTK's own allocation. We replace it with our own 30.

3. **We read our own write back as evidence.** Two call sites do this within a
   handful of lines:

   | file:line | what happens |
   |---|---|
   | `reload_anchor_scroll.rs:620` | `geometry.configure(…)` — writes `upper` |
   | `reload_anchor_scroll.rs:630` | `geometry.is_settled(adjustment.upper(), …)` — **gates** the write on the value just written |
   | `reload_anchor_scroll.rs:642` | `geometry.remember_if_settled(…)` — persists |
   | `centered_scroll_restore.rs:133` | `geometry.configure(…)` — writes `upper` |
   | `centered_scroll_restore.rs:144` | `geometry.is_settled(…)` — classifies on the same value |

   In `settled_row_height`, `adjustment_height = upper / n_rows = 60180 / 2006 =
   30` is compared against the widget modal, which in this trail is also 30. They
   agree → "settled" → `settings::set_row_height(db, 30)`. The wrong value writes
   itself back into the database on every reload.

4. **Every deriver inherits it.** `capture_row_height`
   (`track_list_reload.rs:186`) is literally
   `RowHeight::new(adjustment.upper() / f64::from(old_total))`. Whether it reads
   before or after `configure` decides whether it yields 45 or 30. That is the
   alternation in the trail — **one caller reading a moving value, not two
   callers**:

   ```
   SCROLLMODEL path=anchor.initial.apply … row_height=30.0   (x7)
   SCROLLMODEL path=anchor.initial.apply … row_height=45.0   (x5)
   ```

5. **The reveal pays for it.** `ListLayout::centered_value`
   (`list_geometry_layout.rs:230`) computes
   `row_top(position) + row_height/2 − page_size/2` from the `ListLayout` built
   with that height. With `page=805`:

   ```
   position=1126 → want=33392.5   = 1126×30 − (805−30)/2   ✔ exact
                   should be        1126×45 − (805−45)/2 = 50290
   ```

   Confirmed to the decimal for positions 980, 1061, 1126, 1907 and 184. The
   reveal computes its destination in 30-pixel rows and writes it into content
   laid out in 45-pixel rows.

### Which branch installed the 30, and which one keeps it

The handoff left this open and asked for a measurement run. It is answerable from
the trail's arithmetic alone, and the answer is **both, in sequence**:

- **`contradicting_row_height` installed it.** The transition 45 → 30 is only
  reachable through that function: `upper` was GTK's 90270 (implying 45), the
  widgets stood at 30, and the two *disagreed*. `settled_row_height` returns
  `None` on disagreement — it cannot have written the 30.
- **`settled_row_height` keeps it.** Once 30 is persisted, `configure` seeds
  `upper = 60180`, and now the widget 30 *agrees* with the quotient → "settled" →
  persisted again, every reload, forever.

The fix therefore has to hit both, and it does: strand A deletes the installer
and removes the self-reference that feeds the maintainer.

## 4 — Why the widget measurement does not save us

`settled_row_height`'s contract, quoted from the module doc, is that the
adjustment quotient "is trusted only when it agrees with an independently
measured, uniform set of bound row widgets". Three reasons why that second half
is not independent either:

**(a) Nobody asks the rows what they want.** `ListGeometry::widget_heights`
collects `widget.height()` — the *allocated* height — and nothing else. Every
realized row in the trail reports:

```
SCROLLROWS … first=Some(("GtkColumnViewRowWidget", 30, 31)) distinct_heights=[0, 30]   (x12 of 16)
```

The tuple is `(type, widget.height(), natural from measure(Vertical, -1))`. **30
allocated against 31 natural, on every sample in the trail.** A row allocated
below its own natural height is by definition mid-allocation. That signal exists
and is already read by `scroll_probe::probe_rows` (`scroll_probe.rs:153`) — the
production path never consults it.

Note what the same probe also shows: one line reads `distinct_heights=[0, 30, 45]`.
45 is a real *allocated* widget height. Natural 31 is only the floor the row
content needs, not the list's pitch — so "allocated ≥ natural" correctly accepts
a settled 45 and correctly rejects a squeezed 30.

**(b) "Uniform" is computed over a handful of survivors.**
`RowMeasurement::from_widget_heights` filters to non-zero heights ≥
`ROW_MIN_HEIGHT`, then calls it uniform when `counts.len() == 1`. The trail has
`row_widgets=206`–`207` collapsing to `distinct_heights=[0, 30]` — after the
filter, a small handful of rows certifies the whole list. There is no minimum
sample size.

**(c) The floor is a global CSS token.** `contradicting_row_height`'s only guard
is `widget_height > minimum`, and `minimum` is
`crate::ui::style::tokens::ROW_MIN_HEIGHT = 28`. It cannot tell a settled 30 from
a squeezed 30, and it is nowhere near the real 45. That token's own doc comment
(`tokens.rs:50-59`) already records an earlier instance of this same two-writer
disagreement — 36 against 34 — and the finding that the CSS rule it feeds does
not bind at all.

This is precisely where the predecessor's strand A failed. Its task 2 saw the
self-reference — it wrote *"`upper` is a value this code seeded from the
remembered height, while the measurement comes from realized rows"* — and then
trusted the realized rows without asking whether the allocation had finished. Its
own acceptance test 2a ("assert the persisted value afterwards equals the
allocated one") goes green on exactly this bug, because *the allocated one* is
the mid-flight 30.

## 5 — Why 812 green tests saw none of it

Eleven files reconstruct their expected row height as `adjustment.upper() /
count` and then assert a scroll target computed from it:

```
reveal_track_display_tests.rs:74,84,508      navback_anchor_display_tests.rs:120,154,246
search_viewport_display_tests.rs:187,303,331,429,520
source_switch_centering_display_tests.rs:80,109,158
current_track_selection_tests.rs:211,230,256 delete_follow_display_tests.rs:193
tag_mutation_refresh_display_tests.rs:340,378 start_restore_tests.rs:73
delete_tracks_large_block_display_tests.rs:88,133,183
queue_section_centering_display_tests.rs:171  metadata_navigation.rs:594
```

`upper` is not an independent reading when production has just written it. The
oracle and the code under test share one poisoned source, so a disagreement
between two simultaneous measurements is invisible **by construction** — the same
shape as `test-oracle-derived-from-upper-measures-two-errors`.

The one test that does walk realized widgets,
`row_height_floor_display_tests.rs`, asserts only `ROW_MIN_HEIGHT <= rendered`,
i.e. `28 <= 30`. It passes on the broken build.

**Consequence: a green run of the existing suite is not evidence.** Every new
test in this plan must be shown red on unmodified `origin/dev` first, and strand
A does not land on green gates alone — see its blocking measurement arm.

## 6 — What is *not* the problem

The handoff's **defect 2 — "`PlaybackStarted` never reveals, by construction" —
is discarded.** It describes documented behaviour, not a bug:

- `reveal_policy` maps `PlaybackStarted` and `SessionRestore` to `MarkerOnly`
  (`current_track_selection.rs:39`). The module doc states the intent: *"Row
  activation never moves the viewport; explicit transport centers"* — UX rule
  NAV-10b (`docs/ux-rules.md:3458`, active).
- `PlaybackStarted` has exactly two producers. `player_controller.rs:586` inside
  `play_track_id`, whose only production caller is `queue_transport.rs:432` in
  `play_from_view` — the double-click path, where the row is already under the
  cursor (its three other callers are in `lyrics/lyrics_smoke.rs`). And
  `queue_transport.rs:104`, which picks `PlaybackStarted` only when
  `restored_placement_intact`, else `ExplicitTransport`.
- Every transport path — next/previous, MPRIS, media keys, history, up-next —
  goes through `ExplicitTransport` and does reveal: `mpris_mirror.rs:462`,
  `external_media_neighbours.rs:105`, `playback_history_transport.rs:125,154`,
  `up_next_transport.rs:191,262,345`, `queue_transport.rs:351,571,656,672`.

The trail confirms it from the other side: the reveals *do* fire, five times —
they just land on the wrong number. **No task in this plan touches the reveal
policy.** Changing it would break NAV-10b and orphan a UX rule.

---

## 7 — The shape of the fix

One rule, applied in four places:

> **A row height may only be believed when its evidence came from outside this
> code.**

Mechanically that becomes a single predicate — *has GTK authored an `upper` for
the current row count?* — which strand A introduces once and then uses to decide
three separate things:

| question | answer under the predicate |
|---|---|
| may `configure` write `upper`? | only while GTK has **not** authored one for this `n_rows` |
| what row height does the layout and the reveal use? | `upper / n_rows` once GTK **has** authored it; the remembered height only before that |
| may a height be persisted? | only from a GTK-authored `upper`, and only once the widgets show a finished allocation |

The predicate needs no new signal wiring. `ListGeometryCache` records the
`(n_rows, upper)` of its own last write; when `adjustment.upper()` differs from
that, GTK wrote it. Binding it to `n_rows` preserves the legitimate case the
seeding was built for: right after a model swap GTK's `upper` still describes the
*old* row count, and seeding there is correct.

Two consequences worth stating, because they shrink the change rather than grow
it:

- **`settled_row_height` loses its role in layout.** It is reduced to a single
  question — *may this number go into the database?* The widget measurement no
  longer supplies a height at all; it only witnesses that allocation has
  finished.
- **`capture_row_height` keeps its division.** `upper / old_total` was never the
  wrong formula; dividing an `upper` *we* wrote was the wrong input. Gated on the
  predicate it becomes correct rather than deleted.

Task-by-task detail lives in the strand files.

---

## 8 — Parallelität

The cut is by crate boundary, and no file is touched by both strands.

### Strand A — the loop, and everything that reads it

**Owns:** `crates/reprise-gnome/src/ui/list_geometry*.rs`,
`crates/reprise-gnome/src/ui/track_list/**`

The predicate, the preseed rule, the layout authority, the deletion of
`contradicting_row_height`, the natural-height and sample-size guards,
`capture_row_height`, the new independent-oracle display test, and the repair of
the seven contaminated scroll-target oracles.

All of strand A's verification is inside its own ownership: the predicate and the
guards are GTK-free unit tests in `list_geometry.rs` (the module's own doc notes
that its acceptance arithmetic "remains GTK-free and directly unit tested"), and
the rest are display tests under `track_list/`.

### Strand B — the persisted number and the record

**Owns:** `crates/reprise-core/src/library/settings_geometry.rs`,
`crates/reprise-core/src/db.rs`, the new
`crates/reprise-core/src/library/settings_geometry_migration_tests.rs`,
`docs/plans/the-table-follows-the-music-again.md`

Schema v80 clearing the poisoned setting, and the correction to the predecessor
plan's record. Its tests are migration tests over a synthetic v79 database and
need nothing from strand A.

### Merge order: **A, then B**

Not interchangeable, and the reason is the whole point of the plan. If B lands
first, the next launch of the still-broken build re-persists a wrong height into
the freshly cleared key and the migration has bought nothing.

Note what this ordering also means for acceptance: **strand A alone must already
fix the reported symptom on the poisoned live database.** The persisted 30 seeds
only while GTK has stayed silent for that row count; once GTK writes 90270 the
layout uses 45 regardless of what the database says. B cleans up after the fix —
it is not part of it. That is what makes strand A's blocking measurement arm
meaningful before B exists.

### Post-merge cross-checks

Every comparison below reads across the strand boundary and therefore **does not
happen inside either strand**:

1. **`SUPPORTED_SCHEMA_VERSION` against the live database.** After both land,
   open the user's database once and confirm `user_version = 80` and
   `ui.row_height` absent. Strand A cannot check this; strand B cannot produce
   the app that does it.
2. **One reveal after the migration.** Launch the merged build against the
   (now cleared) live database and reveal a row at position ≥ 1000. It must land
   centred from a cold start with no persisted height, which is a different path
   from strand A's own arm — that one ran against a database still holding 30.
3. **`scripts/check-merge-readiness.sh` on merged `dev`**, once, after both.
4. **UX rule traceability.** No task touches NAV-10b (section 6), so no rule
   should be orphaned — confirm the traceability gate agrees on merged `dev`
   rather than assuming it.

### Why not three strands

Tasks 4–6 of strand A look separable from 1–3 — arithmetic versus consumers —
but the consumers' display tests only go green **once the predicate exists**, so
a "consumers" strand would sit red in its own worktree by construction. That is
the exact failure mode this section exists to prevent. Strand B's two items are
both small and both outside `reprise-gnome`; splitting them further buys no
wall-clock.
