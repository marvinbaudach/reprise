---
slug: the-dormant-scan-progress-test-asks-the-sidebar-slot
worktree: /home/marvin/Projects/reprise-the-dormant-scan-progress-test-asks-the-sidebar-slot
branch: feature/the-dormant-scan-progress-test-asks-the-sidebar-slot
phase: reviewed
codex_session:
created: 2026-09-11
---
# The dormant scan progress test asks the sidebar slot

**Complaint.** `dev` is red on the display shard. Every promotion is blocked
until this is fixed forward.

## Evidence

Run [34577226909](https://github.com/marvinbaudach/reprise/actions/runs/34577226909)
on `3130e2ad70` (#924), job `Display tests 2/4` (103192398796):

```
test ui::scan::scan_progress::tests::set_5_dormant_scan_progress_reserves_no_preferences_space ... FAILED
thread '…' panicked at crates/reprise-gnome/src/ui/scan/scan_progress.rs:691:9:
assertion failed: !view.widget().is_visible()
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3136 filtered out
```

The `Quality gate` failure of the same run is only the aggregator
(`Display was selected but its suite result was failure`). The #923 run's red
`Display tests 3/4` was a pacman mirror outage, not this test — #923 itself
never executed it, because the display shard is skipped on PRs
(`a-new-display-test-first-runs-on-dev`).

## Why the test fails

#923 ("The Devices section rests on the sidebar floor") moved the ownership of
a dock card's `visible` state into `SidebarActivitySlot`:

- `sync_revealer_visibility` (`sidebar_activity_slot.rs:106-116`) keeps the
  revealer **and its child** in step with `reveals_child() || is_child_revealed()`,
  driven by the `reveal-child` and `child-revealed` notifications the slot
  connects at dock time (`track_progress_visibility`, lines 96-103).
- The duplicate `connect_child_revealed_notify` in `ScanProgressView::new()`
  that used to hide the revealer after the crossfade was removed — "one owner
  for one invariant". The constructor keeps `revealer.set_visible(false)` as
  the *initial* state; `begin_visibility` (`scan_progress.rs:431-432`) still
  sets it `true` on `show()`; nothing in the view sets it back to `false`.

The test builds a bare `ScanProgressView`, never docks it, and asks the
revealer whether it is hidden after `finish()` + the minimum-visible hold + the
crossfade. Nobody answers that question for an undocked card any more, so the
last assertion fails. The test is asking the wrong object.

Only one production site constructs a `ScanProgressView`, and it docks the
card in the same breath (`window/window.rs:225-232` →
`sidebar.append_scan_card`). The two other `ScanProgressView::new()` calls
outside the scan module (`main_cover_download_progress.rs:136`,
`scan_worker.rs:389`) are tests. So the bare view never leaks a visible empty
revealer in the running app — **this is a test-only fix**, and the diff must
stay test-only.

The name is stale on top: `set_5_…reserves_no_preferences_space` dates from
the time the card was a second top bar of the Preferences dialog. SET-5 is a
rule about Preferences page spacing and is separately covered by
`set_5_preferences_short_pages_expand_from_the_top`
(`preferences_window.rs:361`). The card lives in the sidebar dock now, and the
rule that says "fully inactive progress cards occupy no space; only active or
still-fading-out cards take part in the layout" is **FB-8**
(`docs/ux-rules.md`, FB-8 paragraph near line 1143).

## What the failing test claims today, and where each claim goes

| # | assertion (`scan_progress.rs:650-693`) | owner after #923 | destination |
| --- | --- | --- | --- |
| 1 | `!view.widget().is_visible()` right after `new()` | slot (initial state still set by the constructor) | slot test, asserted on the **docked** card |
| 2 | after `show()`: `widget().is_visible()`, `reveals_child()` | view (`begin_visibility`) | view test keeps `reveals_child()`; slot test asserts child + revealer visible |
| 3 | after `show()`: spinner spinning, percent `"50%"`, fraction `0.5`, cancel visible **and focusable** | view | view test (unchanged) |
| 4 | `finish(); finish();` → `reveals_child()` still true, spinner stopped, cancel hidden | view (min-visible hold, idempotent finish) | view test (unchanged) |
| 5 | after hold + crossfade: `!reveals_child()` | view (`finish` schedules the collapse) | view test, via a condition wait |
| 6 | after hold + crossfade: `!widget().is_visible()` | **slot** | slot test: revealer hidden, child hidden, slot requests and is allocated 0 px again |

Nothing is dropped; claim 6 moves to the object that owns it, claim 1 is
re-asserted where it is meaningful.

## Decisions (grilled 2026-09-11)

1. Two tests, one per owner — no accessors on the view, no production line.
2. The slot test runs on the bare `SidebarActivitySlot`, not a full `Sidebar`;
   the sidebar level is already pinned by the two `fb_8_*idle*` tests of #923.
3. Oracle for "occupies no space": the slot's **request** (`measure`) *and*
   the **allocation** of a hugging wrapper built like the real bottom region.
4. Condition wait with a deadline in both tests; no fixed sleep, no
   `MIN_VISIBLE_TIME` visibility widening.
5. Names: `fb_8_a_finished_scan_card_occupies_no_space_again` (slot) and
   `finish_holds_the_card_for_the_minimum_visible_time_then_collapses_it`
   (view, no rule prefix). `check-ux-traceability.sh` proves it.
6. No mid-fade probe.
7. One strand.
8. Codex delivers filtered evidence and commits; the unfiltered suite and the
   mutation proof are the orchestrator's.

## Design

Two tests, one per owner. No production line changes.

### A. The slot test — the round trip nobody pinned

`crates/reprise-gnome/src/ui/sidebar/sidebar_activity_slot.rs`, `mod tests`,
next to `device_and_scan_activity_stack_in_stable_bottom_slot_order`. The
module already constructs `SidebarActivitySlot` directly and imports the real
`ScanProgressView`; the full-`Sidebar` variant (DB + window) is not needed.

`fb_8_a_finished_scan_card_occupies_no_space_again` — display test,
`#[ignore = "requires a display; run via xvfb-run"]`, the module's
`if gtk4::init().is_err() { return; }` guard, and
`crate::ui::style::install_css_string_for_test(&crate::ui::scan_card_css::css())`
like `doc_5e`, so the 8 px dock margin is the real one:

1. `let slot = SidebarActivitySlot::new(); let view = ScanProgressView::new();
   slot.set_scan_card(view.widget());` — dock a **dormant** card, as
   `window.rs` does.
2. Build a **hugging wrapper** the way the sidebar's bottom region is built
   (`sidebar_issues_section.rs:42-46`): a vertical `gtk4::Box` with
   `set_vexpand(false)` and `set_valign(gtk4::Align::End)`, containing
   `slot.progress_widget()`. Put the wrapper into a 240×470 region and a
   presented window exactly as `measured_job_card` does (the card must be
   mapped, or the crossfade never runs and the test proves less than
   production shows). Drain with the module's `pump()`.
3. **Dormant.** Assert `!view.widget().is_visible()`, the revealer's child is
   not visible, `slot.progress_widget().measure(gtk4::Orientation::Vertical, 240)`
   reports minimum **and** natural `0`, and `wrapper.height() == 0`. Two
   oracles on purpose: the request is what the bottom region hugs; the
   wrapper's allocation is the number #923 measured at 95 px in the live app.
   (`progress_widget().height()` itself is meaningless here — its `vexpand`
   spacer fills whatever it is given.)
4. **Active.** `view.show(&ScanProgress::Scanning { processed: 2, total:
   Some(4), current_path: "/music/song.flac".into() })`, drain. Assert
   `reveals_child()`, child visible, revealer visible, the slot's natural
   height `>= JOB_CARD_HEIGHT_PX` (85), and `wrapper.height() > 0` — the
   card takes part in the layout.
5. `view.finish()`. Assert **immediately** that `reveals_child()` is still
   true: the 700 ms hold must be observed before the wait, or the hold is
   untested.
6. Wait **until** `!view.widget().is_child_revealed()` — a condition wait
   with a deadline, mirroring `wait_until`/`wait_ms` from
   `scan_edge_line.rs:200-221`: 25 ms `glib::MainLoop` slices ended by
   `timeout_add_local_once`, deadline 5 s, returning the final condition;
   assert the returned bool with a message naming the deadline. Not a fixed
   sleep: `MIN_VISIBLE_TIME` is `pub(super)` to the scan module and a
   wall-clock budget flakes under CI load
   (`settle-is-a-wall-clock-budget-not-a-condition`).
7. **Dormant again.** Drain once more, then assert `!reveals_child()`,
   `!is_child_revealed()`, child hidden, `!view.widget().is_visible()`, the
   slot's minimum and natural height `0`, and `wrapper.height() == 0`.
8. `window.close()`.

Doc comment on the test: this is the FB-8 clause "fully inactive progress
cards occupy no space", measured on the state every scan leaves behind — the
sequence #923 could only verify by hand.

### B. The view test — keep the view's own claims, drop the stale name

`crates/reprise-gnome/src/ui/scan/scan_progress.rs`, `mod tests` (lines
648-693 on `origin/dev`): rework
`set_5_dormant_scan_progress_reserves_no_preferences_space` into
`finish_holds_the_card_for_the_minimum_visible_time_then_collapses_it`:

The name says "minimum visible time", but the display test proves only that a
non-zero hold exists before collapse; the 700 ms arithmetic is covered by the
`remaining_visible_time` unit cases in the same module, and a timing probe is a
non-goal.

- Remove the two `is_visible()` assertions on the revealer (claims 1 and 6,
  including the message "a dormant toolbar progress view must not reserve
  vertical space"). Keep claims 2 (`reveals_child()` after `show()`), 3, 4
  and 5 exactly as they are.
- Replace the fixed `timeout_add_local_once(MIN_VISIBLE_TIME + STANDARD_MS + 50)`
  block and its comment with the same condition wait as in A, on
  `!view.widget().reveals_child()`. `MIN_VISIBLE_TIME` stays in the module
  (`nav_15_…` reads it through `remaining_visible_time`); drop it from the
  test module's `use` only if nothing else there needs it, or clippy will say.
- Keep `#[ignore = "requires a display; run via xvfb-run"]` and the
  `if gtk4::init().is_err() { return; }` guard.

### Non-goals

- Switching the dock cards from `Crossfade` to `SlideUp` (#923 deferred that
  as the owner's call).
- Finding which code path let the revealer's flag drift in the live app (#923
  left that open explicitly; the fix made it layout-irrelevant).
- Any change to `sync_revealer_visibility`, `track_progress_visibility`, the
  `ScanProgressView` constructor, `begin_visibility` or `finish`. If a test in
  this plan cannot be made green without touching production code, stop and
  report — the diagnosis above says it can.
- A mid-fade probe for the "still-fading-out cards take part in the layout"
  half of FB-8. It is timing-bound (a 250 ms window on an Xvfb frame clock)
  and would either flake or skip itself silently; that half of the rule is not
  what broke. Reviewers: do not request it.
- Touching `docs/ux-rules.md`: FB-8 already carries the clause the new test
  evidences.

## Tasks (for Codex, in the worktree)

1. Add test A to `crates/reprise-gnome/src/ui/sidebar/sidebar_activity_slot.rs`
   (`mod tests`), with the local `wait_until`/`wait_ms` helpers. The file is
   at 380 lines; the cap is 800.
2. Rework test B in `crates/reprise-gnome/src/ui/scan/scan_progress.rs`
   (`mod tests`).
3. Verify inside the worktree. Evidence is always the `test result:` line,
   never the exit status of a pipe:
   - `cargo fmt --check`
   - `cargo clippy --locked --all-targets -p reprise-gnome -- -D warnings`
   - the two changed display tests, one process each, with the runner's env
     (`scripts/check-display-tests.sh:270-282` is the reference):
     ```
     GSK_RENDERER=cairo GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
       dbus-run-session -- xvfb-run -a \
       cargo test -p reprise-gnome <full::module::path> -- --ignored --exact
     ```
     Full paths:
     `ui::sidebar::sidebar_activity_slot::tests::fb_8_a_finished_scan_card_occupies_no_space_again`
     and
     `ui::scan::scan_progress::tests::finish_holds_the_card_for_the_minimum_visible_time_then_collapses_it`.
     Success is a line `test result: ok. 1 passed;` — `--exact` with a stale
     or partial name runs nothing and exits 0.
   - the sibling dock tests that share a changed module or the show path,
     same invocation, one per process:
     `ui::sidebar::sidebar_activity_slot::tests::doc_5e_every_job_card_docks_at_the_same_place_and_height`,
     `ui::sidebar::sidebar_activity_slot::tests::device_and_scan_activity_stack_in_stable_bottom_slot_order`,
     `ui::sidebar::sidebar_layout_tests::fb_8_progress_region_reaches_split_view_bottom`,
     `ui::sidebar::sidebar_layout_tests::fb_8_idle_job_cards_leave_devices_on_sidebar_floor`,
     `ui::sidebar::sidebar_layout_tests::fb_8_drifted_idle_job_card_leaves_devices_on_sidebar_floor`,
     `ui::preferences::preferences_chrome_placement_tests::fb_9_counterprobe_legacy_toolbar_status_moves_the_content`,
     `ui::scan::scan_progress::tests::mot_2_background_surfaces_fade_in_place_without_layout_motion`.
   - `scripts/check-ux-traceability.sh` — the rename is exactly this gate's
     input; prove SET-5 stays covered and FB-8 gains a test.
   - `cargo test -p reprise-gnome --bins -- --ignored --list | grep -E 'occupies_no_space_again|then_collapses_it|reserves_no_preferences'`
     must list the two new names and not the old one.
   - **Do not** run the unfiltered `cargo test -p reprise-gnome --bins`, and do
     not block on it: the `settle()` family in `device_sync_runtime_tests`
     flakes under load and has twice left finished work uncommitted
     (`codex-blocks-on-the-settle-family-under-load`). Commit with the
     filtered evidence; the unfiltered gate is run by the orchestrator.
4. Commit in two focused commits (A, then B) with prose subjects in the
   repo's style; no agent attribution lines.

## Verification by the orchestrator (after `coded`)

- Unfiltered `cargo test -p reprise-gnome --bins` once in the worktree
  (through `heavy-run heavy --`), reading the `test result:` line — the
  filtered pass is not evidence for thread-affine tests.
- **Mutation proof, on a committed tree** (`git checkout --` restores HEAD, so
  commit first): remove the `revealer.set_visible(should_be_visible)` branch
  from `sync_revealer_visibility` (`sidebar_activity_slot.rs:108-112`) and run
  test A — it must go red at the dormant-again
  `!view.widget().is_visible()` assertion. The `child.set_visible(...)` branch
  cannot be the mutation target because once the revealer itself is hidden,
  `child.is_visible()` reports false and the enclosing box skips the hidden
  revealer during measurement. Restore. That is the exact mechanism #923
  fixed, so a test that stays green under it measures nothing.
- Control arm for B: on `origin/dev` the old test is red at line 691; the
  reworked test is green on the same code — the difference is the removed
  slot-owned assertion, nothing else.
- Land with `land.sh` right after these gates; dev is red until then, and the
  display shard that proves the fix only runs on `dev`. Watch that dev run.

## Parallelität

The cut exists on paper and is **not taken** (grill decision 7).

- Strand A would own `crates/reprise-gnome/src/ui/sidebar/sidebar_activity_slot.rs`
  (test A); strand B would own `crates/reprise-gnome/src/ui/scan/scan_progress.rs`
  (test B). The file groups are disjoint, there is no merge-order
  dependency (the condition wait removes the only shared symbol,
  `MIN_VISIBLE_TIME`), and no post-merge cross-check would be needed.
- Why single strand anyway: ~60 lines of test code split across two worktrees
  means two full `reprise-gnome` builds and two landings for one red-dev
  fix-forward, each landing costing a ~45 min CI run — and the display shard
  that proves the fix runs only after the merge, so two PRs would just double
  the window in which dev stays red. Wall-clock is the point of a cut, and
  here it goes the wrong way.

One strand, one worktree, one PR.
