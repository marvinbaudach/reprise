---
slug: search-reload-blocks-the-main-thread
worktree: /home/marvin/Projects/reprise-search-reload-blocks-the-main-thread
branch: feature/search-reload-blocks-the-main-thread
phase: planned
codex_session:
created: 2026-08-23
---
# #640 — the cleared search blocks the main thread for 94–120 s

Base `origin/dev` = `1a8cd1cac7`. Diagnoses and removes the pathology behind
#640. It deliberately stops before FB-10's yield loop and before #411's
indicator; both get their own plans, written from the numbers this one
produces. See *Success criterion* for when each of those becomes writable.

---

## What is already established — do not re-derive it

Wave 2 strand B measured the reload on 2026-08-22 (release build, loaded 8-CPU
machine, `stress-100k` and `mixed-sources-128` fixtures, five samples per row).
Those numbers are recorded in FB-10 and in #640. The three that matter here all
end in **the same state — the Library source, no filter, 100,000 rows**:

| Transition | old → new rows | SQL count | ready-to-paint |
|---|---|---:|---:|
| source switch → Library | 0 → 100,000 | 1.5–2.7 ms | **44–64 ms** |
| sort change | 100,000 → 100,000 | 1.7–2.2 ms | **437–671 ms** |
| cleared search | 100 → 100,000 | 1.3–2.4 ms | **94,445–120,006 ms** |

Also established:

- The whole reload chain is synchronous on the GTK main thread —
  `set_filter_and_reload` → `reload_with_anchor_and_viewport` → `run_query` →
  `TrackListModel::set_query_browsed_ai_inner`. No thread, no `glib::spawn`, no
  `async` anywhere in it.
- The counting SQL is never the cost. 2.4 ms is its worst measured value across
  every profile and cause.
- FB-10 [planned] carries the owner's decision of 2026-08-23: 250 ms
  ready-to-paint is the threshold, the reload becomes **interruptible** rather
  than merely announced, a busy state that cannot repaint is prohibited, and
  *"the long cases are therefore defects and are tracked as such"*.

## What is **not** established — and this plan exists because of it

### 1. Nobody knows where the 94 seconds are

#640's body says "the synchronous model rebuild and list projection", but that
is the name of the bracket the instrument happened to draw, not a located cost.
The bracket is exactly: `displayable_us` starts on entry to `run_query`
(`track_list_reload.rs:534`) and stops after `apply_empty_state`. Inside it,
every Reprise-side step has a cost that can be bounded by reading it:

| Step in `run_query` | Static cost |
|---|---|
| `ListGeometry::remember_if_settled` | O(1) + one cached DB write |
| `query_track_count_browsed(_ai)` | **measured**, ≤ 2.4 ms |
| state write + `state.cache.clear()` | O(1) — the cache holds ≤ 8 windows |
| `self.items_changed(0, old, new)` | **GTK, unbounded from here** |
| `apply_queue_header_factory(shared, false)` | no-op off the Queue source |
| `browse_filter_count::update` | unrestricted ⇒ returns `count`, no query |
| `apply_empty_state` | stack-page selection, O(1) |
| trail records + `tracing::info!` | O(1) |
| `(shared.on_reload)(…)` | **a no-op closure in the measuring harness** |

So the 94 s is inside `items_changed` or inside a step whose cost the source
does not reveal. That is a very different statement from "rebuilding a 100,000
row list costs 94 s", and the difference decides what gets built.

### 2. The numbers were measured against an object production never uses

`TrackListModel` is split on `cfg(test)`:

- `track_list_model.rs:182-191` — under `cfg(test)` the `glib::wrapper!`
  declares `@implements gio::ListModel` **only**; the production build also
  declares `gtk4::SectionModel`.
- `track_list_model.rs:554` — the `sections_changed` emission after every query
  swap is `#[cfg(not(test))]`.

`GtkColumnView` decides from exactly that interface how its `GtkListItemManager`
tracks tiles. Wave 2 strand B measured through
`cargo test --release … -- --ignored`, so **every number above was produced by a
model with a different interface set than the running application has.** The
94 s may be understated or overstated in production, a cause found under
`cfg(test)` may be an artefact of the test-only shape, and a production-only
path cannot be seen from there at all.

This is not a caveat to note. It is closed in task 2 before any cause is named.

### 3. The oracle: any hypothesis must explain all three numbers

Three transitions land on the same 100,000-row state and differ by a factor of
**~2000**. A cost inherent to projecting 100,000 rows would appear in all three.
It does not. Two further facts sharpen this:

- The strand-B harness **never iterates the main loop between reloads**, so
  nothing deferred to `idle_add_local_once` or `timeout_add_seconds_local` — the
  `end_of_results` refresh (`track_list_builder.rs:326`), the row-loss watchdog
  (`row_loss_watchdog.rs:74`) — is part of any of these numbers. Task 2's harness
  does run the loop, which is one more reason to expect the numbers to move.
- The cheap path is the only one that resets GTK's tracked state *before* the
  swap: `set_source_and_reload` calls `shared.selection.unselect_all()` and
  `adjustment.set_value(0.0)` (`track_list_reload.rs:459-463`). The sort path and
  the cleared-search path do neither.

That yields two falsifiable hypotheses. The point of tasks 3–4 is to kill at
least one of them, not to assume either:

- **H1 — carried GTK state.** The cost scales with what `gtk4::MultiSelection`
  and `GtkColumnView`'s anchor/focus must carry across the swap, not with the row
  count. Predicts: resetting selection and adjustment in front of the
  cleared-search swap collapses 94 s to tens of ms.
- **H2 — a re-entrant window-query storm.** `set_query_browsed_ai_inner` clears
  `state.cache` and then emits `items_changed`; if GTK synchronously pulls
  `item(position)` over a wide range, every pull is a cache miss running
  `query_track_window` with a large `LIMIT/OFFSET` against a sorted 100k table.
  Predicts: an `item()` call counter shows hundreds to thousands of calls for the
  cleared search and single digits for the source switch.

They are not exclusive and neither may be right.

## Why this plan stops before the indicator and the yield loop

FB-10 decided the reload becomes interruptible. That decision stands and is not
reopened here. What this plan disputes is the *order*:

1. A yield budget cannot be sized against a number that is 2000× off its own
   siblings, and that was measured against a model shape production never has.
2. FB-10 itself calls the long cases defects. An interruptible 94-second reload
   is still a 94-second reload — the window stays live and the user still waits
   two minutes.
3. If the pathology falls, the sort case (437–671 ms) may fall with it. The
   interruptibility work may then be far smaller than it looks today, and #411's
   indicator has a different shape.

## Success criterion

Two stages, and they are not the same question.

- **Stage 1 — the pathology is gone.** The three transitions to the same
  100,000-row end state land **within one order of magnitude of each other**.
  This is the oracle; it is independent of any threshold and survives any host
  load. Reaching it is what makes this plan successful.
- **Stage 2 — #640 is closable.** No reload cause exceeds FB-10's **250 ms**
  ready-to-paint.

If stage 1 is reached and stage 2 is not, **the plan has succeeded** and the
residual is the input to the follow-up plan for the yield loop and Cancel. Say
so plainly in `## Result`; do not widen this plan to chase stage 2.

## Measurement policy

- Stage-1 numbers (the asymmetry) may be measured under any host load. A factor
  of 2000 survives a busy machine.
- Stage-2 numbers (anything held against 250 ms) require a quiet host: hold a
  `wake-lock`, record `loadavg` immediately before and after every run, and
  **discard any run whose load changed materially during it**. A number that
  decides a threshold is not allowed to depend on a neighbouring session.
- Every recorded number carries its arm, its host load, and its exact command.

---

## Preconditions (executed by the session before `/code`, not by Codex)

The instrument this plan builds on exists but was never landed: commits
`06ffa25903` and `0df58c12d9` on `feature/issue-backlog-wave-2-b`, based on
`ada027270a`. Codex is sandboxed to its own worktree and cannot reach that one,
so the session cherry-picks them onto the strand branch after
`worktree.sh ensure`, before starting Codex.

**Verified on 2026-08-23 against `origin/dev` = `1a8cd1cac7`:**

- `06ffa25903` applies cleanly (3 files, +375/−55).
- `0df58c12d9` conflicts on exactly one path, `docs/plans/issue-backlog-wave-2-b.md`
  (modify/delete — the plan doc is not being carried over). Resolution:
  `git rm docs/plans/issue-backlog-wave-2-b.md`, then `cherry-pick --continue`.
- Result: +395/−55 across `diagnostic_trail.rs`, `track_list_model.rs`,
  `track_list_reload.rs`, and nothing else.

Both commits are carried as they are, so the history reads *instrument →
instrument corrected*. Codex is told which parts of them task 1 replaces.

---

## Task 1 — make the instrument tell the truth

Three defects were found in review on 2026-08-22. **No number from this
instrument may be quoted again until all three are closed.**

1. **`displayable_us` does not measure what its name says.** It stops when
   `run_query` returns, not when GTK has painted. It approximates the user's
   wait only for as long as the reload is synchronous — precisely what this issue
   family is about. Move the stop to an actual frame (the `ColumnView`'s frame
   clock — `add_tick_callback` / `connect_after_paint`) and rename the field to
   what it then measures. Recording **both** a work-done span and a
   frame-on-screen span is acceptable and probably better, provided both names
   are honest and the gap between them is explained.
2. **Its guard test proves nothing about runtime.**
   `production_reload_finishes_measurement_after_the_list_is_displayable`
   compares `str::find` offsets in the *source text* of `track_list_reload.rs` —
   it asserts that two substrings appear in that order in a file. Delete it and
   replace it with a test that runs a reload and asserts on the recorded event,
   established from observable values.
3. **`query_us = 0` on the queue path is indistinguishable from a real zero**
   and averages into statistics as one. Make "no query ran" a distinct value
   (`Option`, or a documented sentinel the renderer prints as such).

**Acceptance.** A displayless test asserts the recorded cause, row count and
durations, including the queue path's "no query ran" case. One mutation proof per
defect: break it → red, restore → green, with the exact command and both counts.

## Task 2 — measure the production model shape

Close finding 2 above. The measurement must run where `cfg(test)` is **off**, so
that `TrackListModel` implements `gtk4::SectionModel` and emits
`sections_changed` exactly as it does for a user.

An `--example` cannot do this: `reprise-gnome` has no `[lib]` target (binary-only,
`Cargo.toml:10-12`), and examples reach production code only by `#[path]`-including
leaf modules (`examples/row_loss_dump_repro.rs:18,21`). `TrackList` hangs off the
whole `ui` tree and is not includable that way.

**Build a dev hook in the real binary instead**, following the idiom the
repository already has: `track_list_smoke.rs` drives `REPRISE_SMOKE_SOURCE` /
`REPRISE_SMOKE_QUIT` inside the running `reprise`. Add a sibling — an
environment-gated hook that drives the three transitions of the oracle against
the loaded library, records the trail, and exits. It must:

- run with the real main loop, so each transition is measured to a real frame;
- keep the app's behaviour identical when the variable is unset — an inert hook,
  like the existing one;
- print in a form that can be parsed into the tables this plan asks for, with the
  cause, the row counts, and the host's `loadavg`.

**Acceptance.** The hook's command is quoted, and one run's output for all three
transitions is recorded in `## Result`. A mutation proof that the hook drives
production `set_filter_and_reload`/`reload` and not a copy. State explicitly
whether the 94–120 s reproduces in the production model shape — **if it does not,
say so plainly and give the numbers you did see.** That would be a first-class
finding, not a failure.

## Task 3 — bisect the bracket

Instrument *inside* `run_query`, at a granularity that names a step rather than
a span:

- a per-step duration for each row of the table in section 1 above, recorded
  under one reload id so the steps of one reload can be summed and compared
  against the whole;
- a counter and a cumulative duration for `TrackListModel::item()` and for
  `queries::query_track_window`, reset per reload — this decides H2;
- the counts describing the transition: `old_total`, `new_total`, the number of
  selected items at swap time, and the adjustment's `value`/`upper` — this
  decides H1.

Keep it inside the existing diagnostic-trail seam. Do not add an environment
variable beyond task 2's hook; the repository already has `REPRISE_LOG`. The
instrument observes — it must not reorder, skip or defer any step of the reload.

**Acceptance.** A displayless test that one reload's per-step durations sum to no
more than the whole and that the per-reload counters reset. A mutation proof that
the `item()` counter is wired to the production call, not to the test's own.

## Task 4 — a reproduction small enough to iterate on

The 100k fixture takes minutes to build and the freeze takes two more per sample.
Nobody debugs in that loop. Build a reproduction that:

- seeds its own database **relative to now** — never a pinned date; a date-pinned
  fixture is a time bomb and this repository has been burnt by exactly that;
- runs the three transitions of the oracle to the same end state;
- completes in under a minute and reports the **ratio** between the three, not
  only their absolute values;
- is `#[ignore]`d or hook-gated like the other measurement harnesses, so the
  display gate does not swallow it, and is named so that it is findable.

Find the smallest row count at which the asymmetry is still unmistakable — at
least one order of magnitude between the cheapest and the dearest — and record
it. If the asymmetry does not appear below 100k, say so and keep the full
fixture: that is a finding about where the cost turns on, not a failure.

**Acceptance.** The harness is committed, its command quoted, one run's output
and the ratio recorded in `## Result`.

## Task 5 — name the cause, in one sentence, before touching anything

Write into `## Result`:

- the single step (function, file, line) holding the majority of the block;
- **how much** of it that step holds — a share, from task 3's numbers;
- why that step is cheap in the source-switch transition and expensive in the
  cleared-search one, in terms of the state each carries in;
- which of H1 / H2 / neither this confirms, and what refuted the others.

**A hypothesis that cannot explain all three transitions is not the cause.** If
the numbers do not converge on one step, say that instead of picking the largest
and calling it the cause; "the cost is spread across N steps, here they are" is
still the thing #640 needs.

**No production behaviour may change before this section is written.** This is
the second time this issue family reaches for a fix ahead of a located cause,
and the first attempt is why FB-10 had to be rewritten.

## Task 6 — remove the pathology

Fix the cause named in task 5. Constraints:

- Surgical, and following from task 5. If the fix is not obviously implied by
  the named cause, stop and report rather than widening it.
- **The reload's contract does not change here.** It stays synchronous, it stays
  atomic (the list keeps its previous content until the replacement is ready, per
  FB-10), and no query moves to a thread.
- **TAG-1, SEARCH-9 and NAV-10b are inviolable.** A reload is navigation-neutral;
  an untouched list captures no anchor; a cleared query returns to `pre_search`;
  a new result set reads from its top; a pending reveal or a running glide owns
  the viewport. If the cause can only be removed by changing selection or scroll
  semantics — for instance by hiding an `unselect_all()` in the cleared-search
  path — **task 6 stops and reports that as its finding**, together with what the
  change would cost. Not one test of those three rules may be edited, adjusted or
  relaxed to make a measurement green.

**Acceptance.** The existing display and displayless tests for TAG-1, SEARCH-9
and NAV-10b pass unchanged — list them by name with their counts. A mutation
proof that the fix sits in the production path and not only in the harness.

## Task 7 — re-measure, both arms

Re-run task 4's harness and task 2's production hook **on two arms**: the fixed
build, and as the control the same build with the task-6 change reverted. Two
arms or the number means nothing.

Record in `## Result`: the three transitions per arm, five samples each,
min/median/max, the host's load state per the measurement policy, and the exact
commands. State plainly:

- whether **stage 1** is reached — the three transitions within one order of
  magnitude;
- whether **stage 2** is reached — no cause above 250 ms;
- what the sort case (437–671 ms before) now costs.

If any number got *worse*, report it. A fix trading 94 s of cleared search for
2 s of typing is not a fix, and this is where that shows up.

## Task 8 — hand FB-10 and #411 a real number

One section, no code:

- the residual ready-to-paint cost of every cause, against FB-10's 250 ms;
- which causes still cross it, and by how much;
- what that implies for the interruptibility work: which step would have to
  yield, whether it can be split into resumable units at all, and whether Cancel
  has anything to cancel — FB-10 permits offering it only where it genuinely
  cancels;
- whether FB-10's threshold or wording needs revisiting now that its numbers came
  from a model shape production does not have. **Propose** the change; do not
  edit `docs/ux-rules.md` in this plan.

---

## Acceptance (whole plan)

1. Tasks 1–4 land before any behaviour change, and `## Result` records the
   commands and counts for each.
2. Task 5's cause is named with its share of the total and explains all three
   transitions in the oracle.
3. Task 7 reports both arms. Never a single-arm number, never an extrapolation;
   a measurement that was not taken is reported as missing.
4. `cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D warnings`,
   and the displayless GNOME suite with **fresh XDG data/cache/config roots**
   (`REPRISE_AUDIO_SINK=fakesink`). Record pass/fail/ignored counts — a green run
   with a suite missing is not green.
5. Every mutation proof states the exact command, the red count, the green count,
   and that the mutation was reverted.

## Forbidden

- Adding a spinner, progress bar, status text or any busy affordance. That is
  #411, and it is out of scope until task 8 has produced its number.
- Moving the query or the projection to another thread, or introducing a yield
  loop. That is the follow-up plan.
- Changing `SEARCH_DEBOUNCE_MS` or GTK's `search-delay`. A debounce moves the
  block; it does not shorten it.
- Editing `docs/ux-rules.md`. FB-10 is `[planned]`; its revision is task 8's
  proposal, not this plan's edit.
- Editing or relaxing any test of TAG-1, SEARCH-9 or NAV-10b.
- Quoting any number from `feature/issue-backlog-wave-2-b` as current once
  task 1 changes what the instrument measures.

---

## Parallelität

**No cut. One strand.** Not because the tasks are few, but because they are a
chain that converges on the same files.

- **The change surface is unknown until task 5.** Task 6 edits whatever step
  task 5 names, and the candidates span `track_list_reload.rs`,
  `track_list_model.rs`, `track_list_builder.rs` and possibly `reload_restore.rs`.
  Any ownership declared up front would be a guess, and a guessed boundary is how
  a strand ends holding correct work it is not allowed to commit.
- **Tasks 1–4 all edit `diagnostic_trail.rs`, `track_list_reload.rs` and
  `track_list_model.rs`.** Splitting "fix the instrument" from "bisect the
  bracket" would put two agents in the same three files.
- **The tasks depend on each other in sequence.** Task 2 needs task 1's honest
  stop; task 4 needs task 2's production shape; task 5 needs task 3's counters;
  task 6 needs task 5; task 7 needs task 6. A cut here buys merge conflicts, not
  wall-clock.

**Owned files** (the whole strand, no sub-ownership):

```
crates/reprise-gnome/src/ui/track_list/diagnostic_trail.rs
crates/reprise-gnome/src/ui/track_list/track_list_reload.rs
crates/reprise-gnome/src/ui/track_list/track_list_model.rs
crates/reprise-gnome/src/ui/track_list/track_list_builder.rs
crates/reprise-gnome/src/ui/track_list/track_list_smoke.rs
crates/reprise-gnome/src/ui/track_list/reload_restore.rs
crates/reprise-gnome/src/ui/track_list/*_tests.rs   (only those it adds or fixes)
docs/plans/search-reload-blocks-the-main-thread.md
```

Anything outside this list that task 5 names as the cause is reported back before
it is edited, not silently added.

**Post-merge cross-checks:** none. Nothing here reads a file the strand does not
own. #411's indicator work compares against task 8's numbers and is a separate
plan by construction, not a deferred comparison.

---

## Result

### Task 1 — truthful reload timing

The diagnostic now records two honestly named spans: `work_done_us` ends when
Reprise's synchronous reload work returns, while `next_frame_us` ends on the
`ColumnView`'s next frame-clock tick. Their difference is the main-loop/frame
handoff; neither field claims that pixels have reached physical display
hardware. `query_us` is optional and renders as `none` when the Queue path did
not run a count query.

The runtime guard drives the real `TrackList` reload and observes the diagnostic
event. Its command for every arm below was:

```sh
dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d) \
  XDG_CACHE_HOME=$(mktemp -d) XDG_CONFIG_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  cargo test -p reprise-gnome --bin reprise \
  ui::track_list::diagnostic_trail::tests::production_reload_records_the_real_frame_rows_cause_and_optional_query \
  -- --ignored --exact --nocapture
```

Host load was not sampled for these correctness-only mutation arms; it is
therefore missing rather than inferred.

- Next-frame stop mutation: production code recorded the event synchronously.
  Red: 0 passed, 1 failed, 0 ignored. The failure saw two reload events before
  the main loop could advance. The mutation was reverted. Green: 1 passed,
  0 failed, 0 ignored.
- Runtime-path mutation: production `run_query` reported `count + 1`. Red:
  0 passed, 1 failed, 0 ignored; the observed Library event had `rows=2`
  instead of the seeded one row. The mutation was reverted. Green: 1 passed,
  0 failed, 0 ignored.
- Optional-query mutation: production's Queue arm returned
  `Some(Duration::ZERO)` instead of `None`. Red: 0 passed, 1 failed, 0 ignored;
  the observed event rendered `query_us=0`. The mutation was reverted. Green:
  1 passed, 0 failed, 0 ignored.

The pure display-free timing arithmetic test command was:

```sh
cargo test -p reprise-gnome --bin reprise \
  reload_measurement_records_work_query_and_later_frame_honestly -- --nocapture
```

It passed 1 test, failed 0 and ignored 0. Host load was not sampled because no
performance threshold is read from this test.
