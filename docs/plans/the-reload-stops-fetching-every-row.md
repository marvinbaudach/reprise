---
slug: the-reload-stops-fetching-every-row
worktree: /home/marvin/Projects/reprise-the-reload-stops-fetching-every-row
branch: feature/the-reload-stops-fetching-every-row
phase: shipped
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

## Result

### T3 — the reveal's hold survives the split reload

Every track-list display/ignored test under `ui::track_list::` was run
against the fixed tree, one process per test. The prefix now matches 94
tests, not the 93 first recorded here: `diagnostic_trail_tests.rs`'s
`measure_generated_library_reload_latency` sits under the same module path,
but its ignore reason is "measurement", not "requires a display", and it
opens the real library at `db::default_path()` rather than a seeded fixture —
it is not one of the `*_display_tests.rs` files this scope covers, it fails
the same way with or without every commit on this branch, and running it
against a live 254 MB library four times was already one time too many.
Excluded, unrelated, left alone.

Of the 93 in scope, 92 passed and one failed on the tree as first committed
here (T1 + T2 only):

- `ui::track_list::track_list_reload::reveal_track_display_tests::
  nav_10b_reveal_intent_outranks_later_restore_writers` — red with T2's split
  emission, green on the control arm. A real regression, not a pre-existing
  flake — confirmed, then fixed below.

Mechanism, established with `REPRISE_SCROLL_PROBE=1 --nocapture` on the
failing test (env-gated instrumentation already in the tree; no probe code
was added or removed to get this trace):

```
SCROLLWRITE writer=centered.reveal.seed want=6210.0 from=0.0 upper=243.0 page=243.0
SCROLLUPPER writer=anchor.configure want=9000.0 from=243.0 value=0.0 page=243.0
SCROLLWRITE writer=centered.reveal.instant want=6210.0 from=6210.0 upper=9000.0 page=243.0
SCROLLTO writer=centered.reveal.anchor position=138 from=6210.0 upper=9000.0 page=243.0
SCROLLWRITE writer=hold want=6210.0 from=0.0 upper=9000.0 page=227.0
```

The test drives two reloads in one main-loop turn, both inside
`window::library_shell::route_to_place`: the sidebar's `refresh_and_select`
triggers `TrackList::set_source`, whose `center_playing_track_in_view` writes
the reveal's centred destination (6210.0, lines 1–4 above, via
`centered_scroll_restore::write_centered`); `route_to_place` then calls
`restore_browser_place_with_viewport` for the `RevealTrack` destination
itself, a second, ordinary `ReloadViewport::PreserveAnchor` reload. That
reload's own `reload_anchor_scroll::apply()` reads the adjustment, finds it
already at the reveal's destination, and correctly stands down
(`restore_intent::deliberate_destination_outranks_with_intent`) rather than
overwrite it — the reveal is authoritative. But standing down used to call
`hold.release_now()`, on the assumption that nothing further needed
protecting. The second reload's own split `items_changed(0, old, 0)` /
`items_changed(0, 0, new)` still runs after that decision (shrinking that
walk is T2's whole point, not something this reload skips), and GTK's list
base clears the adjustment's range to nothing in between: line 5 above shows
the value at 0.0, `from=0.0`, claimed by no probed writer — the
`centered.reveal.anchor` label the failing test's own trail attached to that
0.0 was `viewport_steps`' attribution artefact (it labels an observed value
with whichever probed writer ran most recently, not whichever one caused it),
not a real write GTK-anchor made. The single pre-T2 emission never passed
through an empty intermediate state, so this reset never happened and nothing
needed to catch it.

A previous pass at this bug suspected `track_reveal.rs:242`'s generation
check — the retry that `reveal_position` schedules self-aborting because the
second reload bumped `TrackListModel`'s generation first. That is a real
event, but not the seam: the fix below does not touch it, and the retry firing
or not makes no difference, because the hold is armed with the reveal's value
*before* `run_query` runs (`reload_with_anchor_and_viewport`) and covers the
position independently of whether any retry ever runs.

Fix, at the seam that owns the intent (`reload_anchor_scroll.rs`, `apply()`'s
stand-down branch): a restore writer that stands down for a deliberate reveal
destination is asserting that the destination is authoritative, so it must
keep guarding it through the very reload it is reacting to, not release
protection on the assumption the value already survived. `hold.release_now()`
is replaced with `hold.set_target(shared.scroll_glide.deliberate_destination())`
when a destination exists (falling back to the previous release when it does
not, which the branch that reaches this code cannot produce, but keeps the
match total rather than assuming). With the hold re-armed instead of released,
it catches the reset GTK makes and rewrites 6210.0 — line 5 above is that
correction. `RestoreIntent::PostSaveSortAnchor` never reaches this branch
(`deliberate_destination_outranks_with_intent` returns `false` for it
unconditionally, added when that intent landed), so the change is scoped to
`RestoreIntent::PreserveViewport`, the intent this bug lives in.
`release_after(SCROLL_ADJUSTMENT_HOLD)` still bounds how long the re-armed
hold lingers and `MAX_CORRECTIONS` still bounds a fight with another writer —
this does not reopen the runaway-hold failure mode `adjustment_hold.rs`'s own
doc comment warns about.

With the fix, all 93 in-scope tests were re-run (one process per test): 92
passed, one failed —
`queue_section_centering_display_tests::nav_10b_glide_centres_a_queue_row_after_all_section_headers`,
reproducibly, with a row-height measurement mismatch unrelated to reveals or
reloads. Checked per the plan's own T3 rule: red on this tree, red with only
the `reload_anchor_scroll.rs` fix reverted, and red with T2 itself reverted
(`git revert --no-commit b5c312d8da`, restored afterwards, tree left clean).
Red on every arm, including the one that predates this feature entirely —
`git diff origin/dev...HEAD --stat` never lists
`queue_section_centering_display_tests.rs` or (before this fix)
`reload_anchor_scroll.rs`. A pre-existing flake, named and left alone.

Acceptance bullet 2 ("all track-list display tests pass, or every red one is
shown red on the control arm too") is met: 92 of 93 in-scope tests pass, and
the one that does not is red on every arm, including the one before this
feature existed.

### T4 — both arms, release binary, `REPRISE_SMOKE_RELOAD_ORACLE=rows:100000`

Seed: a copy of the pristine 100,000-track fixture from #640
(`~/.cache/reprise-search-oracle-pristine`, schema 78, `PRAGMA quick_check`
ok), copied fresh into `target/oracle-run/<arm>-<n>/` for every sample — the
oracle hook itself only checks the row count, it does not seed. Command per
the plan, `target/release/reprise` built via `cargo build --release
-p reprise-gnome --bin reprise`.

Call counts (identical across all three samples of each arm, so shown once):

| transition | arm | item_calls | window_calls |
|---|---|---:|---:|
| source-switch | fixed | 205 | 1 |
| source-switch | control | 205 | 1 |
| sort-change | fixed | 205 | 1 |
| sort-change | control | 100205 | 201 |
| cleared-search | fixed | 205 | 1 |
| cleared-search | control | 100205 | 201 |

Ready-to-paint (`next_frame_us`), milliseconds, three samples per cell,
loadavg taken immediately before each sample (fixed: 1.16/1.06/0.98; control:
3.10/3.01/2.93 — a concurrent build was running on this host during the
control samples):

| transition | arm | sample1 | sample2 | sample3 | median |
|---|---|---:|---:|---:|---:|
| source-switch | fixed | 30.2 | 30.3 | 31.0 | **30.3** |
| source-switch | control | 29.7 | 30.0 | 29.1 | **29.7** |
| sort-change | fixed | 41.2 | 32.4 | 35.6 | **35.6** |
| sort-change | control | 250.4 | 248.0 | 241.5 | **248.0** |
| cleared-search | fixed | 33.0 | 29.8 | 29.8 | **29.8** |
| cleared-search | control | 243.3 | 244.0 | 246.0 | **244.0** |

The call-count collapse is the primary evidence and is exact and reproducible
across all three samples of each arm (Acceptance bullet 3, met). The timing
medians corroborate it at roughly the same order of magnitude #640 measured
(458/271 ms and 271/… ms before/after that change) even though the host was
not quiet for the control samples; source-switch, which never walked, is
unchanged between arms as expected.

The fixed tree was rebuilt once more after the control-arm swap to prove the
source was back (`git diff HEAD --stat` empty before the rebuild); the binary
hash differs from the first fixed-arm build (rustc/linker embed
build-path/timestamp metadata), which is expected and not evidence of a code
difference — the source tree, not the binary bytes, is what git confirms.

T1's red number, reconfirmed on this same control-arm tree: `fb_10_full_
replacement_fetches_only_the_viewport` still fails with `item_calls=20205`
(20,205 in the commit body; this run: 20205, i.e. no drift).

### Acceptance

- T1's test: red on the control arm (20,205 item_calls, reconfirmed), green
  with T2. Met.
- All track-list display tests pass, or every red one is shown red on the
  control arm too: **met**. 92 of 93 in-scope tests pass;
  `nav_10b_reveal_intent_outranks_later_restore_writers` was red only with the
  fix and is fixed in `reload_anchor_scroll.rs` (T3 above);
  `queue_section_centering_display_tests::
  nav_10b_glide_centres_a_queue_row_after_all_section_headers` is red on every
  arm, including the one before this feature existed, and is named as a
  pre-existing flake, not fixed here.
- T4's table shows the call-count collapse on sort-change and cleared-search:
  met, exactly (100205/201 → 205/1, both transitions, all three samples).
- Fixed-arm medians: sort-change 35.6 ms, cleared-search 29.8 ms — **both well
  under 250 ms**. On the profile #640 measured against, the wait FB-10 (and
  #411) describe no longer exists in the fixed arm. With T3's regression
  fixed and every acceptance bullet met, this PR closes #411's *measured*
  complaint.
