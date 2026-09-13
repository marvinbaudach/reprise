---
slug: the-reload-stops-fetching-every-row
worktree: /home/marvin/Projects/reprise-the-reload-stops-fetching-every-row
branch: feature/the-reload-stops-fetching-every-row
phase: planned
codex_session:
created: 2026-09-13
---
# The reload stops fetching every row

Refs #411 (and #640, closed). Whether this closes #411 is decided by the
numbers in T4 — see *Acceptance*.

## Why

#411 reports that the track browser's own reloads (cleared search, sort
change) show no busy indicator while they block. #640 measured the block on
the release binary with a 100,000-track profile: 271 ms for a cleared search
or a sort change, of which 258.8 ms is one `items_changed` emission, inside
which GTK calls `item()` **100,205 times** and those calls issue 201
synchronous SQL window queries (194.6 ms). A source switch into the same
100,000 rows costs 46 ms and only 205 `item()` calls. #640 stopped there:
"the emission is atomic, so no busy state can repaint (FB-10 (3)), and
splitting it would give up the atomic replacement TAG-1's anchoring relies
on". It closed with the indicator half left to #411.

The 100,000 calls have a specific cause, and it is not the atomic replacement
as such. `gtk_list_item_manager_model_items_changed_cb`
(gtk/gtklistitemmanager.c, current main) does this after
`remove_items`/`add_items`:

```c
  /* Check if any tracked item was removed */
  for (l = self->trackers; l; l = l->next)
    if (tracker->widget && tracker->position >= position
        && tracker->position < position + removed) break;
  /* At least one tracked item was removed, do a more expensive rebuild
   * trying to find where it moved */
  if (l)
    for (i = 0; i < added; i++) {
      item = g_list_model_get_item (model, position + i);
      widget = gtk_list_item_change_find (&change, item);   /* by item identity */
      ...
```

A tracked item is a materialised row the list base follows (focus, anchor,
selection). Whenever the list is showing rows and a reload emits
`items_changed(0, old_total, new_total)`, a tracked row falls inside the
removed range and GTK walks **every added item** looking for the tracked
widgets' items by pointer identity. `TrackListModel::item()` creates a fresh
`glib::BoxedAnyObject` on every call, so no item can ever match, and the walk
runs to the end of the new range: one `item()` per row, one SQL window per
`WINDOW_SIZE` rows. A source switch from an empty list has no tracked rows and
therefore no walk — which is exactly why it costs 205 calls, not 100,205.

The fix is to not present a full replacement as a range GTK must search:
remove everything, then add everything, as two emissions in the same
synchronous call. The first emission has `added == 0`, so the walk has nothing
to fetch; the second has `removed == 0`, so no tracked item was removed and the
walk does not run. GTK then materialises only the viewport, like a source
switch. Both emissions happen before control returns to the main loop, so no
frame can show an empty list (FB-10: the list keeps its previous content until
the replacement is ready), and Reprise's own anchor restore
(`restore_reload_anchor`, TAG-1) runs after `run_query` returns, against the
complete new row set, exactly as today. GTK-side identity persistence of
widgets or selection was never possible with fresh item objects, so nothing
that works today depends on the single emission.

## Facts to build on

- `crates/reprise-gnome/src/ui/track_list/track_list_model.rs`:
  `set_query_browsed_ai_inner` (~line 423) computes `change` — a
  `ModelChange { kind: Span, position: 0, removed: old_total, added: new_total, … }`
  unless the caller passed a narrower verified change — then
  `self.items_changed(change.position, change.removed, change.added)` and,
  `#[cfg(not(test))]`, `sections_changed(position, n_items)` from
  `query_section_change(change)`. `imp::ModelState.total` backs `n_items()`;
  `state.cache` is the window cache; `item()` at ~line 186 goes through
  `queue_item_at` → window query.
- `ModelChange` / `ModelChangeKind` live in `track_list_model_change.rs`.
- The GListModel contract: after `items_changed(p, r, a)` returns,
  `n_items()` must equal the previous count − r + a. Between the two emissions
  `n_items()` must therefore report **0**, and `new_total` only afterwards.
- The diagnostic trail (`diagnostic_trail.rs`) records `Event::ItemsChanged`
  per emission and a `ReloadBreakdown` with `item_calls` / `window_calls` per
  reload; `measure_item_call` wraps `item()`. Its tests
  (`diagnostic_trail_tests.rs`) and the reload tests
  (`track_list_reload_tests.rs`, `track_list_model_tests.rs`,
  `track_list_model_scalability_tests.rs`) may encode "one `ItemsChanged`
  per reload" — adapt only such assertions.
- The production oracle from #640: `REPRISE_SMOKE_RELOAD_ORACLE=rows:100000`
  (`track_list_smoke.rs`, `SMOKE_RELOAD_ORACLE_ENV_VAR`) seeds 100,000 rows
  into a fresh profile, drives source-switch → sort-change → cleared-search,
  waits for the ColumnView's next frame after each, and prints one
  `REPRISE_RELOAD_ORACLE transition=… … item_calls=… window_calls=…
  next_frame_us=…` line per transition. Exact invocation (from #640's plan,
  `docs/plans/search-reload-blocks-the-main-thread.md`, Task 7):

  ```sh
  RUN=<worktree>/target/oracle-run   # fresh dir per sample; never under /tmp
  timeout 180s dbus-run-session -- xvfb-run -a env \
    XDG_DATA_HOME=$RUN/data XDG_CACHE_HOME=$RUN/cache XDG_CONFIG_HOME=$RUN/config \
    GDK_BACKEND=x11 WAYLAND_DISPLAY= GTK_A11Y=none GSK_RENDERER=cairo \
    REPRISE_AUDIO_SINK=fakesink REPRISE_SMOKE_RELOAD_ORACLE=rows:100000 \
    REPRISE_LOG=error target/release/reprise
  ```
  Read `track_list_smoke.rs` for the exact output fields before relying on
  the line format above.
- FB-10 in `docs/ux-rules.md` (~line 1465, `[planned]`) still quotes the
  "94–120 seconds" figure #640 disproved and the 437–671 ms sort figure from
  the test build; obligations (1)–(3) are the owner's decisions and stay.
- Display tests for the track list live in
  `crates/reprise-gnome/src/ui/track_list/*_display_tests.rs`, all
  `#[ignore = "requires a display; run via xvfb-run"]`; anchoring is proven by
  `glide_reload_display_tests`, `navback_anchor_display_tests`,
  `search_viewport_display_tests`, `source_switch_centering_display_tests`,
  `reveal_track_display_tests`, `tag_mutation_refresh*_display_tests`,
  `delete_follow_display_tests`, `queue_section_*_display_tests`,
  `track_list_reload_display_tests`.

## Tasks

### T1 — the red test

A display test (ignored with the display marker) next to the other reload
display tests, named `fb_10_full_replacement_fetches_only_the_viewport`:
build the real track list through `track_list_builder` on a seeded DB of
20,000 tracks, show it in a window, wait for a frame so rows are materialised,
then trigger a sort change (a full replacement, 20,000 → 20,000). Read the
`ReloadBreakdown` for that reload from the diagnostic trail and assert
`item_calls < 1_000` (a viewport is a few hundred rows; the pathology is
tens of thousands). On dev this must fail with `item_calls` ≈ 20,205 — run it
red first and put the observed number in the commit body. If the test-build
model (which lacks `SectionModel`) makes the count differ, report the number
you see; the assertion still separates the two arms by two orders of magnitude.

### T2 — the fix

In `set_query_browsed_ai_inner`, for a `Span` change that replaces the whole
model — `position == 0 && removed == old_total && added == new_total &&
old_total > 0 && new_total > 0` — emit two signals instead of one:

1. set `state.total = 0` (cache already cleared), record
   `Event::ItemsChanged { position: 0, removed: old_total, added: 0 }`, emit
   `items_changed(0, old_total, 0)`;
2. set `state.total = new_total`, record
   `Event::ItemsChanged { position: 0, removed: 0, added: new_total }`, emit
   `items_changed(0, 0, new_total)`, then the production-only
   `sections_changed` for the new range as `query_section_change` computes it
   for the second emission.

Every other shape (partial spans from `changed_range`, block moves, empty →
N, N → empty) keeps the single emission it has today — they are cheap or
already walk-free. Write a doc comment on the branch that says why two
emissions exist, naming `gtk_list_item_manager_model_items_changed_cb` and
the identity-search it performs; the next reader will otherwise "simplify"
it back. Adapt the trail/reload unit tests that asserted a single
`ItemsChanged` per reload, and only those.

### T3 — anchoring still holds

Run every track-list display test (see *Verification scope*). Anything red:
run the same test on the control arm (stash the fix) before concluding — a
test red on both arms is a pre-existing flake, name it and move on; a test
red only with the fix is a real regression and must be fixed here (the likely
cause would be a scroll adjustment reset between the two emissions that
`AdjustmentHold` / `restore_reload_anchor` does not cover — fix at that seam,
not by re-merging the emissions).

### T4 — both arms, on the production binary

Build `cargo build --release -p reprise-gnome --bin reprise` for the fixed
tree, run the oracle three times (fresh `$RUN` each time), and record per
transition: `item_calls`, `window_calls`, `next_frame_us`. Then stash the fix
(`git stash` of the T2 commit's files via `git revert --no-commit <T2>` or a
checkout of the parent's `track_list_model.rs`), rebuild, run three control
samples, restore the fix, rebuild once more to prove the tree is back. Record
`cat /proc/loadavg` before each sample. The **call counts** are the primary
evidence — they are deterministic and unaffected by host load; the timings
are secondary and the host will not be quiet (other builds run concurrently),
so report them as medians with the loadavg beside them and no claim beyond
the A/B ratio. Expected: sort-change and cleared-search fall from ~100,205
item calls / 201 window queries to a few hundred / ≤ 2, and their
ready-to-paint span drops to the source-switch's order of magnitude.

Put the whole table into `docs/plans/the-reload-stops-fetching-every-row.md`
under a `## Result` heading (commit it with the T4 commit).

### T5 — FB-10's stale numbers

In `docs/ux-rules.md`, FB-10: replace only the measurement sentences ("on a
100,000-track profile a sort change blocked … 437–671 ms and a cleared search
… 94–120 **seconds**" and "The counting SQL never exceeded …") with the
corrected record: the 94–120 s figure was a test-build artefact (#640); the
release binary measured 458 ms before #640, 271 ms after it, and *X* ms after
this change (your T4 median), because GTK's list item manager walked every
row of a full replacement looking for tracked rows by identity, which two
emissions avoid. Keep the 250 ms threshold sentence, obligations (1)–(3), the
2026-08-23 decision paragraph and the `[planned]` status **unchanged** — the
rule text is the owner's; only the numbers it quotes are corrected. Separate
commit, so it can be dropped on its own.

Commits: T1 `The full replacement is measured for every row it fetches`,
T2 `The reload stops fetching every row`, T3 only if a regression needed a
fix, T4 `The two arms are measured on the release binary`, T5
`FB-10 quotes the release-binary numbers`.

## Acceptance

- T1's test is red on the control arm and green with T2.
- All track-list display tests pass, or every red one is shown red on the
  control arm too.
- T4's table shows the call-count collapse on sort-change and cleared-search.
- If the fixed arm's sort-change and cleared-search medians are under 250 ms:
  say so explicitly in the summary — the PR then closes #411, because the
  wait it reports no longer exists on the profile it was measured on. If they
  are not: say that too, with the numbers; the PR then only references #411.

## Out of scope

- The busy indicator itself. FB-10 (3) prohibits one that cannot repaint, and
  after this change the reloads it would announce are expected to sit far
  under the threshold; whether FB-10's indicator obligation stays `[planned]`
  is the owner's decision, informed by T4.
- Off-main-thread prefetching, model restructuring, `WINDOW_SIZE`,
  `MAX_WINDOW_LIMIT`, indexes.

## Parallelität

Not cut. Every task touches or measures `track_list_model.rs`; T3–T5 depend
on T2.
