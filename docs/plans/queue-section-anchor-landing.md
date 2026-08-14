---
slug: queue-section-anchor-landing
worktree: /home/marvin/Projects/reprise-queue-section-anchor
branch: feature/queue-section-anchor
phase: planned
codex_session:
created: 2026-08-14
---
# Landing plan — queue-section-anchor (#444): restore the deleted settled gate

Land `feature/queue-section-anchor`, which fixes issue #444 ("Sectioned Queue
visits the top before restoring its anchor").

The anchor implementation is written and reviewed, and the evidence-repair work
described under "Record" below is done. One display test blocks the landing, and
it is this branch's own production regression. This plan repairs it.

## The blocker

`ui::track_list::track_list_reload::search_viewport_display_tests::typed_search_reads_from_the_top_and_clearing_comes_back`
(`crates/reprise-gnome/src/ui/track_list/search_viewport_display_tests.rs:101`)

```
clearing returns within a row of where the search began: expected about 1200, got 1428
```

Attributed by measurement on 2026-08-14, with a control arm on the same host
minutes apart:

| | result |
|---|---|
| this branch (`ab02785783`), 2 runs | **FAILED, FAILED** — byte-identical message |
| `origin/dev` (`5721ade95e`), 2 runs | **ok, ok** |

Deterministic on both sides, so it is neither herd flakiness nor a host
artefact. The branch introduced it.

## Diagnosis

Commit `1e8c940ed4 fix(queue): make scroll anchors header-aware` changed
`apply()`'s return type from `bool` to `Option<ListLayout>` in
`crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs` and, in the same
edit, deleted three things from the tail of that function:

* **(a)** the `geometry.is_settled(upper, n_rows, n_sections)` gate, and with it
  the `ListGeometry::is_settled` method itself;
* **(b)** the `geometry.remember_if_settled(...)` call;
* **(c)** the `provisional_sectioned_refinement` hold-target deferral.

The commit has no message body, no replacement comment, and none of this
branch's own planning documents mentions `is_settled`. It reads as an unreviewed
side effect of the return-type restructuring, not as a decision.

Without (a), `apply()` reports success on the first synchronous pass after a
model swap, when GTK has realized no `ColumnViewRow` widget yet. Two things
follow, and **either one alone** would have prevented the failure:

1. `scroll_to_anchor` (`reload_anchor_scroll.rs:151-200`) chooses its `scroll_to`
   guard row from `apply()`'s result. On failure it uses the plain
   `request.anchor_position` (row 35, `:179`); on success it uses
   `reload_restore::prepaint_guard_position` (`:170`), which deliberately names
   the row at the **bottom** of the viewport (row 42) — correct only when the
   geometry really is settled. GTK top-aligns row 42: 42 × 34 = **1428**.
2. `schedule()` arms `arm_refinement` only when `applied_layout.is_none()`
   (`:130-131`). That is the sole entry point for the corrective
   `RestorePath::Idle` / `ItemsChanged` / `PageSize` passes, so nothing pulls the
   wrong first shot back to 1200.

Measured with the branch's own `REPRISE_SCROLL_PROBE=1`, both sides are
line-for-line identical until:

```
SCROLLMODEL path=anchor.initial.apply anchor=Some((15, 10.0)) position=Some(35) row_height=34.0 sections=[] target=1200.0   # both
branch:  SCROLLTO writer=anchor.initial.scroll_to position=42  from=1200.0 upper=6800.0 page=239.0
dev:     SCROLLTO writer=anchor.initial.scroll_to position=35  from=1200.0 upper=6800.0 page=239.0
```

### Two things that are *not* the cause — do not change them

* **`prepaint_guard_position` is correct.** Its formula is mathematically
  identical to dev's hand-rolled arithmetic (`ceil((1200+239)/34)-1 = 42`); the
  branch only refactored it onto `ListLayout::last_row_above`. Naming the last
  visible row is required, and its doc comment says why.
* **`ListLayout::validate` / `LayoutValidation::NoOpinion` is not involved.**
  `layout_for_live_allocation` (`track_list_geometry.rs:61-69`) short-circuits
  with `if !layout.has_sections() { return layout; }`, and the failing scenario
  has `sections=[]`. Even if reached, `validate` would return `Accepted`, because
  `upper` was already synthetically preseeded by `geometry.configure(...)` to the
  predicted value — a computed number agreeing with another computed number.
  **Real widget realization is the only signal that separates "preseeded" from
  "settled".**

## Decisions taken in the grill of 2026-08-14 — do not re-open without the user

1. **The earlier grill's decision 2/3 ("production geometry is not touched, the
   oracle gets repaired instead") is lifted.** Its premise was that the
   production arithmetic is right; that premise is refuted for the unsectioned
   path. The fix reaches into production code and lands with this branch, as one
   PR.
2. **All three deletions come back, for both the sectioned and the unsectioned
   path.** A literal revert is the smallest defensible answer to "a gate was
   deleted without a reason", and with the gate back `Some ⟺ settled` holds
   again, so every consumer of `apply()`'s result keys off the right fact with no
   signature change. Restoring only two of three would produce a combination that
   neither dev nor this branch has ever run.
3. **Pre-defined bisection if the sectioned pair goes red** (task 6): first drop
   (c) again and re-measure; only then narrow the gate to `n_sections == 0`.
   Do **not** invent a third behaviour (a struct carrying `layout` + `settled`)
   without returning to the user.
4. **The regression oracle keeps its value assertion and gains a semantic one**,
   conditionally — see task 5.
5. **Evidence = the focused set, then the full display gate**, the gate running
   concurrently with the handover rather than blocking it.
6. **R2 is measured, not assumed** (task 7). If sectioned lists never reach
   "settled" but the sectioned pair is green, that lands with a follow-up issue;
   it does not block. Blocking would mean rebuilding the header-height model,
   which the earlier decision 1 put out of scope and which stays out of scope.
7. **Nothing is sent outward by the pipeline.** No PR is opened and no issue is
   filed; both texts are prepared and handed over.
8. **One strand.** See `## Parallelität`.

## Tasks

Each task is independently checkable. The file list is what the task **may**
touch; nothing else.

0. **Rebase onto `origin/dev`.** *Done before this plan was written* — the branch
   is 11 ahead, 0 behind at `6a54c2316e`, rebased without conflicts. Every
   measurement below therefore runs on the final tree, and no post-rebase re-run
   is owed.

1. **Re-add `ListGeometry::is_settled`.** Restore the method from
   `origin/dev:crates/reprise-gnome/src/ui/list_geometry.rs` (dev `:368-377`),
   placed next to `settled_row_height` (`:366-368`). No visibility change is
   needed anywhere: `section_header_measurement` (`:359-364`) is private to
   `list_geometry.rs` and the restored method lives in that same file. Add one
   sentence of doc comment saying *why* it exists — real widget realization is
   the only signal that distinguishes a pre-seeded `upper` from a settled one.
   Files: `crates/reprise-gnome/src/ui/list_geometry.rs`.

2. **Restore the gate and the persistence call in `apply()`.** In
   `crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs`, after
   `geometry.configure(...)` and **before** `adjustment.set_value(target)`:

   ```rust
   if !geometry.is_settled(adjustment.upper(), current_ids.len(), n_sections) {
       return None;
   }
   geometry.remember_if_settled(/* as dev called it */);
   ```

   Unsettled means `return None` with **no** `set_value` — that is dev's
   semantics: it declined to write a target derived from an unproven row height.
   Take the exact argument list from
   `git show origin/dev:crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs`
   rather than guessing it. Add a comment above the gate naming the failure it
   prevents (a bottom-edge guard row against an unrealized layout), so the next
   return-type refactor cannot delete it silently again.
   Files: `crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs`.

3. **Restore the `provisional_sectioned_refinement` hold deferral**, also from
   dev's version of the same function: a non-`Initial` refinement of a sectioned
   list must not `set_hold_target`. This is deletion (c) and it becomes live
   again the moment the gate is back, because `arm_refinement` starts firing.
   Files: `crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs`.

4. **State the coupling where the guard is chosen.** One comment at
   `reload_anchor_scroll.rs:164-180` recording that `applied_layout.is_some()`
   now means *settled*, and that `prepaint_guard_position`'s bottom-edge row is
   valid only under that condition. No code change. It is a separate task so it
   cannot vanish in diff noise.
   Files: `crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs`.

5. **Add the semantic assertion to the regression test — conditionally.** The
   existing assertion checks the resulting scroll value; the defect is the guard
   *row*. Add a second assertion beside it: after the filter is cleared, the
   topmost row actually visible in the list viewport is the anchor row.
   **Condition:** only if the viewport derivation the queue tests already use
   (derive the viewport top from the scroll adjustment, not from raw widget
   coordinates — see `queue_section_geometry_display_tests.rs`) is reachable from
   `search_viewport_display_tests.rs` **without moving production code and
   without building new infrastructure**. If it is not, change nothing here,
   keep the value assertion alone, and write the gap into `.pipeline-codex.md`
   so it can be filed as a follow-up.
   Explicitly forbidden: building a test-readable sink in `scroll_probe`. That
   module emits only to stderr via `eprintln!`, has no in-process capture and no
   `#[cfg(test)]` helper — measured. Do not add one for a single test.
   Files: `crates/reprise-gnome/src/ui/track_list/search_viewport_display_tests.rs`.

6. **Measure the sectioned pair.** `nav_back_to_a_large_sectioned_queue_never_visits_the_top`
   (`queue_section_geometry_display_tests.rs:508`) and
   `queue_anchor_names_the_row_at_the_viewport_top` (`:623`). Both must report
   `test result: ok. 1 passed`. **This is the decision point.** If either is red,
   apply the bisection of decision 3 — first revert task 3, re-measure; if still
   red, narrow the gate to `n_sections == 0` and re-measure — and record in
   `.pipeline-codex.md` which step made it green.
   Files: `crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs`
   (bisection only).

7. **Measure R2 — does the sectioned path ever reach "settled"?** One run of the
   sectioned pair with `REPRISE_SCROLL_PROBE=1`, looking for an `anchor.*.apply`
   record that follows an initial pass which returned `None`. Report the finding
   in `.pipeline-codex.md`. This turns R2 from an unknown into a known; it does
   not gate the landing (decision 6).
   Files: none.

## The verification, cheapest first

Judge display tests **only** on the `^test result:` line — a name filter that
matches nothing prints `ok. 0 passed`, which is not a pass. Each display test
runs in its own process with its own XDG roots, `dbus-run-session` and
`xvfb-run -a`. Redirect every long output to a log and answer by `grep`; never
read a whole log back.

* **S1 — `cargo fmt --check`, then `cargo clippy -p reprise-gnome --all-targets -- -D warnings`.** Exit 0.
* **S2 — `cargo test -p reprise-gnome --bin reprise`.** No line matching
  `^test result: FAILED`. (`--lib` finds nothing in this crate.)
* **S3 — the blocker, twice.** Two consecutive runs of
  `typed_search_reads_from_the_top_and_clearing_comes_back`, both
  `test result: ok. 1 passed`. Two runs because the failure was byte-identical
  twice; one green run is not symmetric evidence.
* **S4 — the sectioned pair** (task 6). Both `1 passed`.
* **S5 — the remaining focused paths**, one run each, all `1 passed`:
  `navback_anchor_display_tests.rs:307`, `glide_reload_display_tests.rs:57`,
  `reveal_track_display_tests.rs:98`,
  `fresh_start_allocation_display_tests.rs:50`,
  `track_list_reload_display_tests.rs:30`.
* **S6 — the full display gate.** `DISPLAY_TEST_JOBS=1 scripts/check-display-tests.sh`,
  summary `failed: 0`. Runs detached through `heavy-run`, watched by one
  background watcher on process exit / log stall / deadline — never polled.
  It runs **concurrently** with the handover: S1–S5 are the gate to the handover,
  S6 is the gate to the merge, and the merge is the user's.

The baseline this is measured against is not assumed: the full gate ran on
`ab02785783` at 08:33–09:50 on 2026-08-14 and reported **671 of 672 passed**,
the single failure being the blocker above. Every other display test in the
blast radius is therefore known green on this code.

## Risks

| # | Risk | The measurement that disproves it |
|---|---|---|
| R1 | Restoring the gate re-breaks the two sectioned tests, i.e. #444 regresses. | S4. Both green ⇒ disproved. Red ⇒ bisection per decision 3. |
| R2 | The gate never converges for sectioned lists (`headers.is_uniform()` never holds live), so they sit permanently unsettled. | Task 7. Note the claim that headers measure 20/34 came from a fixture without the application stylesheet; with it they measure 36/36, which is the CSS floor `SECTION_HEADER_MIN_HEIGHT = 36`. If R2 is true *and* S4 is green, it is harmless and becomes a follow-up. |
| R3 | The restored corrective pass introduces a visible one-frame jump. | The baseline is not "no jump" but "no worse than dev", since this is dev's shipped behaviour. `nav_back_to_a_large_sectioned_queue_never_visits_the_top` in S4 is exactly the assertion for a top-visit. |
| R4 | Other rows-only display tests that pass today start failing, because the always-applied behaviour was silently compensating elsewhere. | S5 first, S6 as the real answer. |
| R5 | Restoring `remember_if_settled` persists a row height at a moment the branch had stopped persisting one, leaking a cached value into later tests. | S6, and specifically the six `tag_mutation_refresh_display_tests.rs` cases, the densest users of cached geometry. |
| R6 | Restoring the hold deferral (task 3) changes non-`Initial` sectioned refinements in a way no test covers. | S4 and S6. If S4 goes red, decision 3's bisection attributes it in one run. |

## Parallelität

**One strand.** The cut was attempted and rejected with reasons:

* Tasks 1–4, 6 and the bisection all edit
  `crates/reprise-gnome/src/ui/track_list/reload_anchor_scroll.rs` (plus eleven
  lines of `list_geometry.rs`). A second strand would own only task 5.
* That strand could not verify itself. Task 5's new assertion sits in the very
  test that is red until task 2 lands, so it could not go green in its own
  worktree *in principle* — its check would have to move to the post-merge list
  for ~15 lines of test code.
* Task 5 is also semantically downstream: what "the correct topmost row" means is
  what tasks 1–3 define.
* The load governor had 0 of 6 slots free when this plan was written, so two
  concurrent Codex runs would serialize anyway.

The parallelism that does pay needs no cut: S3, S4 and S5 are independent
processes with their own Xvfb servers and run concurrently once the fix compiles.
S6 runs detached alongside the handover.

**Post-merge cross-checks:** none are owed. With a single strand every check
reads files the strand owns, and the rebase already happened before the work
started, so no comparison is deferred past the merge.

## Record — the four corrections that produced the current diff

Kept because they are the reason the diff touches tests at all, and they belong
in the PR body.

**C1 — the red q-journey was never inherited from `dev`.** `RenderedBandSamples`,
`rendered_band_samples` and `uniform_heights` have zero occurrences on
`origin/dev`; they arrived with this branch's own `settle section band
measurement` commit. On `dev` the test only asserts that both header titles
render, and CI ran it green.

**C2 — the 20 px header was not a settling race.** Strengthening the settle
predicate to `has_both_bands() && uniform_heights().is_some()` makes the test run
the full timeout (5.51 s instead of 0.49 s) without ever converging.

**C3 — the fixture never installed the application stylesheet.**

| | rows | section headers | q-journey |
|---|---|---|---|
| without the stylesheet | 34 px | 20 px and 34 px | red |
| with the stylesheet | 45 px | **36 px and 36 px** | green |

36 px is exactly the `section_header_height` the model assumes, and
`style/tokens.rs:66` (`SECTION_HEADER_MIN_HEIGHT: i32 = 36`) is compiled into
`.queue-section-header-row { min-height: 36px; }` in `queue_sections.rs:82-85`.
The model is right and the fixture was wrong.

**C4 — the reference frame, not the anchor, was off.** The scroll viewport starts
at y = 26. The row the oracle picked spanned −19..26 and had zero visible pixels
— a realized virtualization slack row that `.first()` chose because it sorted by
raw y including negatives. Ruled out by the same probe: stale row height,
`headers_above` off by one, and any capture/render frame skew.

## Deferred by decision — do not re-raise here

Duplicate entries in `section_starts` double-counted by `headers_above`; the
unreachable `Option` on `content_height`/`max_scroll`; `rendered_queue_headers`
not filtering zero-height widgets.

## Follow-ups to hand over (not filed by the pipeline)

* **A** — display fixtures that measure without the application stylesheet, plus
  the reference-frame trap (`compute_bounds(&column_view)` includes the
  non-scrolling title bar, so y = 0 is not the viewport top). Ask for a sweep.
* **B** — `validate` treats an `upper` below the prediction as "still growing"
  and returns `NoOpinion`, so it can only ever reject a guess that is too short,
  never one that is too long.
* **C** — whatever tasks 5 and 7 record as a gap in `.pipeline-codex.md`.
