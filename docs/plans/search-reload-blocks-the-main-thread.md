---
slug: search-reload-blocks-the-main-thread
worktree: /home/marvin/Projects/reprise-search-reload-blocks-the-main-thread
branch: feature/search-reload-blocks-the-main-thread
phase: reviewed
codex_session:
created: 2026-08-23
---
# #640 — the cleared search blocks the main thread for 94–120 s

Base `origin/dev` = `890706293f`. Diagnoses and removes the pathology behind
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
(`track_list_reload.rs:575`) and stops after `apply_empty_state`. Inside it,
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

- `track_list_model.rs:189-198` — under `cfg(test)` the `glib::wrapper!`
  declares `@implements gio::ListModel` **only**; the production build also
  declares `gtk4::SectionModel`.
- `track_list_model.rs:567-574` — the `sections_changed` emission after every query
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
  `end_of_results` refresh (`track_list_builder.rs:327-330`), the row-loss watchdog
  (`row_loss_watchdog.rs:79`) — is part of any of these numbers. Task 2's harness
  does run the loop, which is one more reason to expect the numbers to move.
- The cheap path is the only one that resets GTK's tracked state *before* the
  swap: `set_source_and_reload` calls `shared.selection.unselect_all()` and
  `adjustment.set_value(0.0)` (`track_list_reload.rs:497-499`). The sort path and
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

**Superseded premise:** Task 2 found that the production binary's cleared-search
reload was about 430 ms, not 94 seconds. The two-minute-wait argument above is
retained as plan history, but it is not a current premise; see Task 2's result.

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

**Measurement-policy limitation:** every recorded `loadavg` below, including
the endpoints shown as a range, is a single-point sample rather than the required
before/after pair, and no run records a `wake-lock`. These performance numbers
are therefore Stage-1-grade only. In particular, Task 2's statement that sort
and clear were above the 250 ms threshold is not valid Stage-2 threshold
evidence and must not be used as such.

Every file-and-line citation in this plan was re-grepped and re-verified against
HEAD during the #640 review pass.

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

### Task 2 — production-model oracle

The fixture was generated from the existing relative-to-now synthetic metadata
generator. It contains 100,000 rows, including exactly 100 rows matching
`Artist 0000`, at `/tmp/reprise-search-oracle.Z7SGIp`; this disposable path is
not user data. The exact generator command was:

```sh
fixture_root=$(mktemp -d /tmp/reprise-search-oracle.XXXXXX)
mkdir -p "$fixture_root/data/reprise" "$fixture_root/cache" "$fixture_root/config"
cargo run -p reprise-core --release --example scalability_baseline -- \
  --db "$fixture_root/data/reprise/reprise.db" --tracks 100000 --iterations 1
```

Host load was not sampled during generation because generation is setup, not a
reload measurement. The production binary (`cfg(test)` off, including
`gtk4::SectionModel` and `sections_changed`) was then run with this exact
command (the first invocation also compiled the release binary):

```sh
timeout 300s dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=/tmp/reprise-search-oracle.Z7SGIp/data \
  XDG_CACHE_HOME=/tmp/reprise-search-oracle.Z7SGIp/cache \
  XDG_CONFIG_HOME=/tmp/reprise-search-oracle.Z7SGIp/config \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  REPRISE_SMOKE_RELOAD_ORACLE=1 \
  cargo run --release -p reprise-gnome --bin reprise
```

That production arm reported loadavg 3.78 for all three transitions:

| Transition | Rows | Query | Work done | Next frame |
| --- | ---: | ---: | ---: | ---: |
| source switch | 100,000 | 1.679 ms | 49.398 ms | 53.600 ms |
| sort change | 100,000 | 1.698 ms | 412.965 ms | 416.901 ms |
| cleared search | 100,000 | 1.759 ms | 426.675 ms | 430.006 ms |

The 94–120 s cleared-search result does **not** reproduce in the production
model shape. The observed cleared-search result was 430.006 ms, roughly 220×
to 279× smaller. The largest-to-smallest next-frame ratio in this run was
8.02×, so this run already met Stage 1; sort and cleared search remained above
the 250 ms Stage-2 threshold. The frame-clock stop is the next tick of the real
`ColumnView`, not proof that physical pixels reached a monitor.

The hook's production-path mutation replaced its call to
`set_filter_and_reload(&shared, ORACLE_FILTER)` with `reload(&shared)`. The
exact Bash validator command for both arms was:

```sh
oracle_output=$(timeout 60s dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=/tmp/reprise-search-oracle.Z7SGIp/data \
  XDG_CACHE_HOME=/tmp/reprise-search-oracle.Z7SGIp/cache \
  XDG_CONFIG_HOME=/tmp/reprise-search-oracle.Z7SGIp/config \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= GTK_A11Y=none GSK_RENDERER=cairo \
  REPRISE_AUDIO_SINK=fakesink REPRISE_SMOKE_RELOAD_ORACLE=1 \
  REPRISE_LOG=error target/release/reprise 2>/tmp/reprise-task2-oracle.err)
printf '%s\n' "$oracle_output"
test "$(printf '%s\n' "$oracle_output" | \
  rg -c '^REPRISE_RELOAD_ORACLE transition=')" -eq 3
test "$(printf '%s\n' "$oracle_output" | \
  rg -c '^REPRISE_RELOAD_ORACLE error=')" -eq 0
```

Red: 0 passed, 1 failed, 0 ignored (exit 1), with two transition lines followed
by `error=unexpected-filter-count rows=100000 expected=100` at loadavg 2.39.
The production mutation was reverted. Green: 1 passed, 0 failed, 0 ignored
(exit 0), with all three transition lines and no error at loadavg 3.58; its
source/sort/clear next-frame spans were 51.406/421.790/421.438 ms.

### Task 3 — bracket bisection

The diagnostic assigns one `reload_id` to the coarse reload event and its
breakdown. Top-level steps are geometry persistence, count query, state/cache
swap, `items_changed` (including any required production-only section signal),
queue-header work, browse count, empty state, trail/logging and `on_reload`.
Nested counters separately report `ListModelImpl::item` and SQL window-query
calls and cumulative time. The exact production command was:

```sh
timeout 60s dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=/tmp/reprise-search-oracle.Z7SGIp/data \
  XDG_CACHE_HOME=/tmp/reprise-search-oracle.Z7SGIp/cache \
  XDG_CONFIG_HOME=/tmp/reprise-search-oracle.Z7SGIp/config \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= GTK_A11Y=none GSK_RENDERER=cairo \
  REPRISE_AUDIO_SINK=fakesink REPRISE_SMOKE_RELOAD_ORACLE=1 \
  REPRISE_LOG=error target/release/reprise 2>/tmp/reprise-task3.err | \
  rg '^REPRISE_RELOAD_ORACLE transition='
```

All three rows came from this production arm at loadavg 6.29:

| Transition | Whole work | `items_changed` | item calls/time | window calls/time |
| --- | ---: | ---: | ---: | ---: |
| source switch, 0→100k | 46.298 ms | 36.628 ms | 205 / 20.192 ms | 2 / 20.173 ms |
| sort, 100k→100k | 419.914 ms | 411.022 ms | 100,205 / 356.648 ms | 502 / 346.009 ms |
| clear, 100→100k | 416.184 ms | 407.981 ms | 100,205 / 356.889 ms | 502 / 345.285 ms |

For cleared search, `items_changed` held 98.03% of the synchronous bracket;
the nested window queries alone held 82.96% of the bracket. The selection count
was 0 and adjustment value was 0.00 in every transition. Source/sort/clear
adjustment uppers were respectively 132/4,500,000/4,500, but the two expensive
arms had identical call counts despite those different carried ranges.

The display-free breakdown/reset test command was:

```sh
cargo test -p reprise-gnome --bin reprise \
  ui::track_list::diagnostic_trail::tests::reload_breakdown_sums_inside_the_whole_and_resets_per_reload \
  -- --exact
```

It passed 1, failed 0 and ignored 0. Its first synthetic breakdown summed
7.000 ms inside a 10.000 ms whole; the next reload reset both item and window
counters to zero. Host load was not sampled because this is an arithmetic and
reset correctness test, not a performance arm.

For the production counter mutation, the call to `record_item_call` was removed
from `ListModelImpl::item`. Both arms used:

```sh
dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d) \
  XDG_CACHE_HOME=$(mktemp -d) XDG_CONFIG_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= GTK_A11Y=none GSK_RENDERER=cairo \
  REPRISE_AUDIO_SINK=fakesink cargo test -p reprise-gnome --bin reprise \
  ui::track_list::diagnostic_trail::tests::production_reload_records_the_real_frame_rows_cause_and_optional_query \
  -- --ignored --exact --nocapture
```

Red: 0 passed, 1 failed, 0 ignored; the real reload retained one window query
but reported `item_calls=0`. The production mutation was reverted. Green:
1 passed, 0 failed, 0 ignored. Host load was not sampled because this mutation
checks wiring, not a threshold.

### Task 4 — bounded reproduction

The committed production hook accepts its expected row count through its one
existing variable (`REPRISE_SMOKE_RELOAD_ORACLE=rows:<count>`), validates the
loaded count and filtered count, runs the three transitions, prints their
ratio, and exits. The harness combines it with the existing generated-metadata
tool, then rewrites `added_at` from SQLite's current clock; no pinned date is
present. The exact Bash command for the recorded full-size run was:

```sh
case_root=$(mktemp -d /tmp/reprise-reload-relative-now.XXXXXX)
mkdir -p "$case_root/data/reprise" "$case_root/cache" "$case_root/config"
case_started=$(date +%s%N)
target/release/examples/scalability_baseline \
  --db "$case_root/data/reprise/reprise.db" --tracks 100000 --iterations 1 \
  >"$case_root/generator.json"
sqlite3 "$case_root/data/reprise/reprise.db" \
  "UPDATE tracks SET added_at = CAST(strftime('%s','now') AS INTEGER);"
timeout 60s dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME="$case_root/data" XDG_CACHE_HOME="$case_root/cache" \
  XDG_CONFIG_HOME="$case_root/config" GDK_BACKEND=x11 WAYLAND_DISPLAY= \
  GTK_A11Y=none GSK_RENDERER=cairo REPRISE_AUDIO_SINK=fakesink \
  REPRISE_SMOKE_RELOAD_ORACLE=rows:100000 REPRISE_LOG=error \
  target/release/reprise 2>"$case_root/app.err" | \
  rg '^REPRISE_RELOAD_ORACLE (transition=|summary )'
case_ended=$(date +%s%N)
printf 'REPRISE_RELOAD_ORACLE elapsed_ms=%s fixture=%s\n' \
  "$(((case_ended-case_started)/1000000))" "$case_root"
```

It completed in 6.857 s at loadavg 5.98. Source/sort/clear next-frame spans
were 62.569/438.878/451.384 ms, giving a 7.214× ratio. Exploratory runs of the
same hook found ratios of 1.626× at 10k (loadavg 2.56) and 4.002× at 50k
(loadavg 2.68); the 100k repeat was 8.530× (loadavg 2.54). The 1k arm is
missing a clear result: its search setup unexpectedly remained at 1,000 rows,
and the hook correctly stopped with an error instead of inventing a number.

No tested size up to and including 100k reached the task's 10× "unmistakable
asymmetry" threshold. Therefore there is no smaller qualifying reproduction;
the full 100k fixture is retained. Even it completes far under one minute and,
in the production model shape, confirms that the former multi-order pathology
is absent rather than recreating it.

### Task 5 — cause checkpoint

**Cause, in one sentence:** the full-range production
`TrackListModel::set_query_browsed_ai_inner` call to `self.items_changed` at
`crates/reprise-gnome/src/ui/track_list/track_list_model.rs:566` makes GTK
synchronously revalidate a single 100,000-row `SectionModel` section, holding
407.981 ms of the 416.184 ms cleared-search bracket (**98.03%**) while it pulls
all 100,000 positions through the lazy model.

The nested evidence locates the mechanism, not merely its outer signal: clear
and sort each made 100,205 `item()` calls and 502 SQL window calls. For clear,
those window queries consumed 345.285 ms (82.96% of the whole); sort made the
same number of calls and spent 346.009 ms in them. Source switch inserts into
an empty old model (`old_total=0`), so GTK has no carried whole-model section to
replace and asks only for the visible neighbourhood: 205 item calls and two
windows. Sort replaces a nonempty 100k model and clear replaces a nonempty
100-row model, so both enter the same whole-section revalidation path despite
their different old cardinalities. This explains all three transitions.

This confirms **H2**, the re-entrant window-query storm. H1 is refuted: every
arm had zero selected rows and adjustment value 0.00, yet source switch stayed
cheap while sort and clear were expensive; the two expensive arms also had
different adjustment uppers (4,500,000 versus 4,500) but identical item/window
call counts. The cost is therefore not carried selection or scroll position.

No production behavior had changed when this checkpoint was written. The
measurements and harness in Tasks 1–4 were the only production additions, and
they only observe or run when the explicit smoke variable is present.

### Task 6 — bounded lazy windows

The surgical fix increases the lazy SQL window from 200 to 500 rows. GTK still
receives one synchronous, atomic full-range `items_changed` replacement, but
its 100,205 `item()` calls can now share at most 201 window queries instead of
502. The cache remains bounded at eight windows (4,000 rows). Selection,
viewport, debounce and scroll code are unchanged. A trial that divided the
single non-Queue section into 200-row sections left the production call counts
and timings unchanged and was reverted. A 2,000-row window trial emitted GLib
object-lifetime criticals and timed out; it too was reverted before choosing
the conservative 500-row size.

The production-path mutation changed `WINDOW_SIZE` back from 500 to 200. After
each arm, `cargo build --release -p reprise-gnome --bin reprise` rebuilt the
real binary. Both arms then used this exact Bash validator:

```sh
oracle_output=$(timeout 60s dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=/tmp/reprise-search-oracle.Z7SGIp/data \
  XDG_CACHE_HOME=/tmp/reprise-search-oracle.Z7SGIp/cache \
  XDG_CONFIG_HOME=/tmp/reprise-search-oracle.Z7SGIp/config \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= GTK_A11Y=none GSK_RENDERER=cairo \
  REPRISE_AUDIO_SINK=fakesink REPRISE_SMOKE_RELOAD_ORACLE=1 \
  REPRISE_LOG=error target/release/reprise 2>/tmp/reprise-task6-oracle.err)
printf '%s\n' "$oracle_output"
window_calls=$(printf '%s\n' "$oracle_output" | \
  rg '^REPRISE_RELOAD_ORACLE transition=cleared-search' | \
  sed -E 's/.*window_calls=([0-9]+).*/\1/')
test "$window_calls" -le 201
```

Red: 0 passed, 1 failed, 0 ignored (exit 1). The production control made 502
cleared-search window calls at loadavg 7.75–8.08; source/sort/clear next-frame
spans were 51.812/466.885/465.645 ms. The mutation was reverted. Green:
1 passed, 0 failed, 0 ignored (exit 0). The fixed production binary made 201
cleared-search window calls at loadavg 5.24; spans were
39.909/266.033/263.722 ms. Task 7 supplies controlled five-sample timing arms;
these differently loaded mutation arms prove production-path wiring only.

The protected rule tests were not edited. Display-free commands and outcomes
were:

```sh
cargo test -p reprise-gnome --bin reprise tag_1_
cargo test -p reprise-gnome --bin reprise search_9_
cargo test -p reprise-gnome --bin reprise nav_10b_
```

TAG-1 passed 14, failed 0 and ignored 8; SEARCH-9 passed 2, failed 0 and
ignored 0; NAV-10b passed 8, failed 0 and ignored 9. The passing display-free
tests, listed by name, were:

- TAG-1: `tag_1_query_reload_keeps_the_scroll_anchor_from_editor_open`,
  `tag_1_selection_after_save_is_written_tracks`,
  `tag_1_non_sorting_save_keeps_the_original_scroll_anchor`,
  `tag_1_sort_changing_save_reanchors_on_the_first_edited_track`,
  `tag_1_anchors_on_the_first_track_that_can_actually_move`,
  `tag_1_plain_library_rating_save_is_viewport_neutral_in_place`,
  `tag_1_rating_dependent_views_and_tag_writes_still_requery`,
  `tag_1_positions_for_ids_maps_surviving_ids_only`,
  `tag_1_deleted_ids_drop_silently`,
  `tag_1_prepaint_target_resolves_the_stable_anchor_before_offset_restore`,
  `tag_1_reanchoring_on_an_edited_row_preserves_its_screen_offset`,
  `tag_1_scroll_target_follows_anchor_row_after_resort`,
  `tag_1_reanchoring_counts_a_header_between_the_two_rows`, and
  `tag_1_scroll_target_none_when_anchor_gone`.
- SEARCH-9: `search_9_debounce_is_the_only_wait` and
  `search_9_filter_change_decides_viewport_by_the_new_query`.
- NAV-10b: `nav_10b_one_marker_implementation_serves_every_list_surface`,
  `nav_10b_every_list_surface_places_the_marker_the_same_way`,
  `nav_10b_the_radio_marker_reapplies_without_rebuilding_the_model`,
  `nav_10b_a_paused_radio_keeps_the_loaded_marker_but_freezes_its_motion`,
  `nav_10b_a_foreign_write_ends_the_glide`,
  `nav_10b_a_far_target_jumps_instead_of_gliding`,
  `nav_10b_playback_scroll_policy_distinguishes_user_intent`, and
  `nav_10b_reveal_follows_the_track_when_the_view_changes_underneath`.

GTK cannot be initialized on a different test-worker thread in the same
process, so every ignored display test used this exact one-test process shape,
with each fully-qualified name substituted for `$test_name`:

```sh
dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d) \
  XDG_CACHE_HOME=$(mktemp -d) XDG_CONFIG_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= GTK_A11Y=none GSK_RENDERER=cairo \
  REPRISE_AUDIO_SINK=fakesink cargo test -q -p reprise-gnome --bin reprise \
  "$test_name" -- --ignored --exact
```

The display tests passed 18, failed 0 and ignored 0 in aggregate. Listed by
name, these were:

- TAG-1: `tag_1_focus_returning_to_the_table_after_a_save_keeps_the_viewport`,
  `tag_1_restoring_dialog_focus_after_a_save_keeps_the_viewport`,
  `tag_1_save_refresh_requeries_the_view_once`,
  `tag_1_save_refresh_shows_the_written_tag_on_screen`,
  `tag_1_tag_save_refresh_paints_no_frame_at_the_table_top`,
  `tag_1_year_save_keeps_the_edited_album_inside_the_viewport_after_resort`,
  `tag_1_query_reloading_metadata_save_keeps_the_live_viewport`, and
  `tag_1_reload_with_a_deep_anchor_keeps_a_row_inside_the_viewport`.
- SEARCH-9: `typed_search_reads_from_the_top_and_clearing_comes_back`.
- NAV-10b: `nav_10b_a_reload_does_not_count_as_the_user_scrolling`,
  `nav_10b_deleting_the_running_track_keeps_the_follow_to_the_next_one`,
  `nav_10b_a_user_scroll_during_the_glide_wins`,
  `nav_10b_centering_lands_on_the_logical_pixel_nearest_the_target`,
  `nav_10b_row_activation_marker_does_not_move_selection_or_viewport`,
  `nav_10b_a_scan_reload_mid_glide_does_not_strand_the_follow`,
  `nav_10b_glide_centres_a_queue_row_after_all_section_headers`,
  `nav_10b_player_bar_title_centers_in_one_viewport_step`, and
  `nav_10b_player_bar_title_centers_the_revealed_track`.

Host load was not sampled for these correctness suites because no performance
threshold is read from them. An initial six-test TAG-1 process produced
1 pass/5 failures, and an initial eight-test retry produced 1 pass/7 failures,
all solely because GTK rejected initialization from successive Cargo
test-worker threads; neither failed run is reported as green, and the isolated
exact-test invocations above are the accepted results.

### Task 7 — not run

The controlled five-sample fixed/control arms remain outstanding. They require
a quiet host, a `wake-lock`, and before/after load samples for every run; this
machine does not currently satisfy that measurement policy. The execution
handover is recorded in `.pipeline-codex.md`.

### Task 8 — not run

The FB-10/#411 residual-cost handover remains outstanding because it depends on
Task 7's valid Stage-2 measurements. The execution handover is recorded in
`.pipeline-codex.md`.

### Whole-plan acceptance — not run

Whole-plan acceptance remains outstanding because Tasks 7 and 8 have not run,
so the required two-arm threshold evidence and its downstream interpretation do
not exist. The execution handover is recorded in `.pipeline-codex.md`.
