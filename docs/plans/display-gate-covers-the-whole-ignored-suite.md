---
slug: display-gate-covers-the-whole-ignored-suite
worktree: /home/marvin/Projects/reprise-display-gate-covers-the-whole-ignored-suite
branch: feature/display-gate-covers-the-whole-ignored-suite
phase: coded
codex_session:
created: 2026-08-13
---
# The display gate covers the whole ignored suite

> Every line number and every measurement in this plan is against
> `a10cd59d60` (`origin/dev` at the time of writing). Read the source from that
> revision, not from the stale main checkout, which is 261 commits behind.

## Why

`scripts/check-merge-readiness.sh` runs the display suite through three name
filters — `--rule-named`, `--motion`, `--css` — and never as `mode=all`. Every
`#[ignore]`d test whose function name matches none of them is in **no standing
gate at all**.

Measured on 2026-08-13 against `a10cd59d60`, by running exactly the complement
of those three filters under the gate script's own worker isolation:

| | |
|---|---|
| ignored tests discovered by `cargo test -p reprise-gnome -- --ignored --list` | 654 |
| covered by `--rule-named` | 458 |
| covered by `--css` | 9 (2 of them also rule-named) |
| additionally covered by `--motion` | **0** |
| **in no standing gate** | **185** |
| of those: green, each with its own `test result: ok. 1 passed` line | **182** |
| of those: red | **3** |

The 185 are not edge cases. They include
`closing_the_row_context_menu_leaves_the_library_viewport_where_it_was`,
`a_recycled_list_row_drops_the_previous_cover_immediately`,
`column_headers_update_sort_state_and_reload_once` — the exact regression
classes this project has repeatedly bled on. They also include **both
regression tests written for the blank-track-list work** that shipped on
2026-08-13 (packages A and B, PRs #445 and #449): those tests protect nothing
today.

Two of the three red tests have been red since a deliberate product change
landed and nobody noticed, because no gate ever ran them. That is the cost of
the gap, and it is the reason this plan closes it rather than merely widening
the filter list.

## What is wrong, precisely

### 1. `--motion` is dead weight

`check-display-tests.sh:39-48` selects `^mot_[0-9]+[a-z]?_`. Its comment
justifies the extra gate step by saying the MOT section of `docs/ux-rules.md`
is not committed, so `--rule-named` would filter every motion test out.

That is no longer true. `docs/ux-rules.md` at `a10cd59d60` contains 8 `MOT-…`
rules, so `mot` is in the prefix set `--rule-named` derives at
`check-display-tests.sh:56-59`, and all 30 `mot_*` tests are already selected by
it. The `--motion` step re-runs 30 display tests for no coverage gain, and its
comment documents the opposite of what the code now does.

### 2. `handle_queue_drop_dispatches_ids_to_the_wired_callback` — an orphaned expectation

`crates/reprise-gnome/src/ui/sidebar/sidebar_tests.rs:112-129` drops
`[Track(7), Episode(7)]` into `handle_queue_drop` and asserts the callback sees
both. It sees `[Track(7)]`.

That is correct behaviour, not a defect.
`sidebar_dnd.rs:223-229`'s `queue_drop_tracks` filters episodes out on purpose,
introduced by `3524bf9c3f` — *"Podcasts and YouTube episodes stop being queue
citizens (#314)"*, landed 2026-08-06. The test kept the pre-#314 expectation.
Red in 3 of 3 dedicated re-runs.

Note that `sidebar_dnd.rs` already carries a non-display unit test for the
filter itself (`episode_payload_is_never_reinterpreted_as_a_colliding_playlist_track`),
so the display test's job is the *dispatch wiring*, not the filter policy.

### 3. `preferences_are_a_dialog_with_a_page_sidebar` — asserting on the wrong level

`preferences_window.rs:551-556` walks `PAGE_ORDER` and requires
`stack.child_by_name(id.name())` to carry the CSS class
`reprise-preferences-page`.

Preference pages are materialised lazily. `stack.child_by_name(…)` returns the
`adw::Bin` **holder** (`preferences_window.rs:150-168`); the class is added to
the page that becomes the holder's *child*, and only when the page is first made
visible. The holder never carries the class, so the assertion cannot pass for
any page that has not been visited — and for the visited one it still checks the
wrong widget. Red in 3 of 3 dedicated re-runs.

The lazy materialisation arrived with the `materialize_page` work (#342 on
2026-08-07 / #370 on 2026-08-08); this plan does not need to attribute it more
precisely than that.

### 4. `nav_back_to_a_large_sectioned_queue_never_visits_the_top` — a real one-row settle

This is the only one of the three that is about product behaviour.

`queue_section_geometry_display_tests.rs:311-316` requires the restored
viewport to settle within one row height of its anchor. Measured across **23
dedicated runs** (3 + 20), it failed 22 times, every failing run reporting
byte-identical numbers:

```
QUEUEPROBE headers=["Play Next", "Now Playing"] rows=2276 row_h=34.0
  expected=37489 final=37454
  samples(n=73 first=Some(37454.0) min=37454 max=37488)
```

Read the samples, not just the endpoint:

- `first=37454` — immediately after the restore the viewport is already one row
  above the anchor.
- `max=37488` — at some point during the 600 ms window it **is** at the expected
  position (37489), i.e. the correct offset is computed and written.
- `final=37454` — and then it is taken back, by exactly one row height (34.0).

So the position is wrong, corrected, and then un-corrected. `arm_refinement`
(`reload_anchor_scroll.rs:85-173`) installs **two** refinement paths — one on
`items_changed`, one via `idle_add_local_once` — and `refine_once`
(`:175-201`) both writes the pixel-exact offset through `apply` and then calls
`scroll_to_anchor`, which asks GTK for `scroll_to(guard_position, …,
ListScrollFlags::NONE, …)`. GTK's `scroll_to` establishes *just visible*, not
*exactly at this offset*, and `prepaint_guard_position`
(`reload_restore.rs:120-132`) picks that position through a `ceil()`.

**Which of those steps takes the correction back is the open question, and it
must be measured before anything is changed.** Two dry-paper explanations were
tried during planning and both turned out wrong: a section-header height model
(refuted — the expected value is actually reached, so the model is right) and
plain herd flakiness (refuted — 22 of 23 runs, identical numbers).

The second assertion of the same test, "never visits the top"
(`:306-310`), holds in every single run: `min` never drops below 37454.

**A warning for whoever measures this.** The blank-track-list review raised
exactly this call — `refine_once` calling `scroll_to_anchor` on the
already-allocated paths — as its one HIGH finding, and it was dismissed by a
two-arm probe with two runs per arm (see
`docs/plans/2026-08-13-session-handover.md`). Against a test that is red 22 of
23 times, two runs per arm cannot separate the arms. That dismissal is not
evidence. Re-run any arm comparison with **at least 20 runs per arm** and
compare the `QUEUEPROBE` numbers, not just pass/fail.

## What to do

Four strands. 1–3 are independent and should run in parallel; 4 depends on all
of them. Each strand owns its own files; only strand 4 touches the two shell
scripts, so no two strands can collide there.

### Strand 1 — the queue-drop expectation (owns `sidebar_tests.rs`)

Bring the test in line with the #314 decision: the callback receives only the
track items. Keep the test's real subject — that the wired callback is reached
and that `handle_queue_drop` reports success — and make the episode filtering an
explicit, named expectation rather than an incidental one, so the next reader
sees a decision rather than a mismatch.

### Strand 2 — the preferences assertion (owns `preferences_window.rs`)

Assert the class where it actually lives: on the materialised page inside the
holder. The test must keep proving what it was written for — every page in
`PAGE_ORDER` gets the styling hook — which means it has to drive the
materialisation for each page rather than assuming eager construction. Do not
make production eager to satisfy the test; the lazy path is the shipped
behaviour and is deliberate.

### Strand 3 — the one-row settle (owns the track-list restore path)

Diagnose first, fix second. Do not start by changing `refine_once`.

1. Instrument the restore so each write to the vadjustment is attributed to its
   source (which of the two `arm_refinement` paths, `apply` vs
   `scroll_to_anchor`). The existing `REPRISE_SCROLL_PROBE` /
   `crate::ui::scroll_probe` hook (`reload_anchor_scroll.rs:217`) is the natural
   place; the test already prints `QUEUEPROBE` on the green path too.
2. From that trace, name the step that moves 37488 back to 37454.
3. Only then change it, and re-measure with **20 runs**, not one.

The acceptance number is the same one the test asserts: the settle must land
within one row height of `expected`. Do not widen the tolerance to make the test
pass — the tolerance is the specification here, and `expected` is provably
reachable because the run already reaches it.

If the diagnosis shows the fault is in the test's model rather than in the
restore, that is an acceptable outcome, but it needs the same evidence: 20 runs
and the trace that shows which write wins.

### Strand 4 — the gate (owns `check-display-tests.sh`, `check-merge-readiness.sh`)

Once 1–3 are green:

- Drop the `--motion` step and its now-false comment from
  `check-merge-readiness.sh`, and drop the `--motion` mode from
  `check-display-tests.sh` unless something outside the gate still calls it
  (check before removing).
- Run the display suite so that **every** discovered ignored test is covered.
  Prefer running it as one `mode=all` invocation over maintaining a growing list
  of name filters: the filters are what created this gap, and any new test whose
  name does not match a rule prefix would silently fall back out of the gate.
  If total wall-clock is the reason the filters exist, say so in the commit and
  raise `DISPLAY_TEST_JOBS` for the gate instead of narrowing coverage — the
  185-test measurement run took about 4 minutes at `DISPLAY_TEST_JOBS=4`.
- Whatever the final shape, the gate must fail if a discovered ignored test is
  in no invocation. A coverage gap must be a red gate, not a silent omission.

## Verification

Per strand, and again at the end:

- `bash scripts/check-display-tests.sh` (full, `mode=all`) must be green.
- Take test names from `cargo test -p reprise-gnome -- --ignored --list`, never
  guessed. A `--exact` filter that matches nothing exits 0 and the gate script
  writes its pass marker anyway — a green result line reading
  `0 passed; N filtered out` is a test that never ran. Require `1 passed`.
- Note that `cargo test -p reprise-gnome` has more than one test binary: a
  single test run legitimately prints one `ok. 1 passed` and one `ok. 0 passed`.
  Count the `1 passed` lines.
- For strand 3, 20 runs, and report the `QUEUEPROBE` distribution rather than a
  pass/fail verdict.
- `reprise-gnome` has no `--lib` target; only `--bin reprise` exists.

## Out of scope

- The two remaining follow-ups from `docs/plans/2026-08-13-session-handover.md`:
  arming self-heal for the `present > 0 / allocated == 0` class, and
  deduplicating the two `ColumnViewRow` walks. Independent of this work.
- The ungelandet branch `fix/dev-gate-repair` (`ba2fa3d7f4`, worktree
  `../reprise-dev-gate-repair`, 2026-08-13 09:55) moves track-list code to
  satisfy `check-frontend-thinness.sh`. It touches neither shell script and
  neither of the three red tests, so it does not conflict — but it is unlanded
  and someone should decide what happens to it.
